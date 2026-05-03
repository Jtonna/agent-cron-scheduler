use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use crate::pty::{NoPtySpawner, PtySpawner};
use crate::workflow::agents::impl_for;
use crate::workflow::step::{Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

pub use crate::models::workflow::AgentStep;

/// Substitute the literal `${prompt}` token in `template` with `prompt_value`.
///
/// This is a distinct second substitution pass, separate from the `${input.*}` /
/// `${steps.*}` pass done by `template::substitute`. A workflow author writes:
///
///   prompt: "review ${input.repo}"
///
/// and the template engine substitutes `${input.repo}` → e.g. "acme/widgets". Then
/// `${prompt}` in the command template is substituted with the resolved prompt
/// string. Two passes prevents recursive/double expansion of `${}` sequences that
/// appear inside the resolved prompt value.
fn substitute_prompt(template_str: &str, prompt_value: &str) -> String {
    template_str.replace("${prompt}", prompt_value)
}

/// Build a `portable_pty::CommandBuilder` for a shell command string.
/// Mirrors the same logic in shell.rs.
fn build_command(command: &str) -> portable_pty::CommandBuilder {
    #[cfg(windows)]
    {
        let mut cmd = portable_pty::CommandBuilder::new("cmd");
        cmd.arg(format!("/C {}", command));
        cmd
    }
    #[cfg(not(windows))]
    {
        let mut cmd = portable_pty::CommandBuilder::new("sh");
        cmd.arg("-c");
        cmd.arg(command);
        cmd
    }
}

/// Parse raw captured bytes into a `serde_json::Value` based on the parser spec.
/// Mirrors the same logic in shell.rs.
fn parse_output(buf: &[u8], parser: Option<&str>, step_id: &str) -> Value {
    match parser {
        Some("json") => match serde_json::from_slice(buf) {
            Ok(v) => v,
            Err(_) => Value::String(String::from_utf8_lossy(buf).into_owned()),
        },
        Some("lines") => {
            let text = String::from_utf8_lossy(buf);
            let lines: Vec<Value> = text
                .split('\n')
                .filter(|l| !l.is_empty())
                .map(|l| Value::String(l.to_owned()))
                .collect();
            Value::Array(lines)
        }
        Some("raw") | None => Value::String(String::from_utf8_lossy(buf).into_owned()),
        Some(other) => {
            tracing::warn!(step_id = %step_id, "unknown capture parser '{}'; treating as raw", other);
            Value::String(String::from_utf8_lossy(buf).into_owned())
        }
    }
}

/// Core execution logic, parameterized over a spawner for testability.
async fn execute_with_spawner(
    step: &AgentStep,
    ctx: &mut StepContext,
    spawner: &dyn PtySpawner,
) -> Result<StepOutput, StepError> {
    let agent = impl_for(&step.agent_type);

    // Step 1: Resolve the prompt — substitute ${input.*} and ${steps.*} references.
    let prompt_sub = template::substitute(&step.prompt, &ctx.input, &ctx.steps);
    for warn in &prompt_sub.warnings {
        tracing::warn!(step_id = %step.common.id, "prompt template warning: {}", warn);
    }
    let resolved_prompt = prompt_sub.output;

    // Step 2: Resolve the command template.
    // First substitute ${input.*}/${steps.*} (user might embed those in a custom template).
    let raw_template = step
        .command_template
        .as_deref()
        .unwrap_or_else(|| agent.default_command_template());
    let template_sub = template::substitute(raw_template, &ctx.input, &ctx.steps);
    for warn in &template_sub.warnings {
        tracing::warn!(step_id = %step.common.id, "command_template warning: {}", warn);
    }
    // Then substitute ${prompt} with the resolved prompt value.
    // This is a separate pass — see substitute_prompt() for rationale.
    let final_command = substitute_prompt(&template_sub.output, &resolved_prompt);

    // Step 3: Build the command.
    let mut cmd = build_command(&final_command);

    // Set working dir and env.
    if let Some(ref dir) = ctx.working_dir {
        cmd.cwd(dir);
    }
    for (k, v) in &ctx.env {
        cmd.env(k, v);
    }

    // Step 4: Write START marker.
    let _ = ctx
        .log_sink
        .write_step_start(&step.common.id, Utc::now())
        .await
        .map_err(StepError::Io)?;

    // Step 5: Spawn the process.
    let mut process = spawner
        .spawn(cmd, 24, 80)
        .map_err(|e| StepError::Spawn(e.to_string()))?;

    // No stdin for agent steps.
    process.close_stdin();

    // Step 6: Stream output with timeout.
    let max_bytes = step.common.capture.stdout_max_bytes;
    let mut capture_buf: Vec<u8> = Vec::with_capacity(std::cmp::min(max_bytes, 65536));

    let child_pid = process.pid();
    let timeout_secs = step.common.timeout_secs.filter(|&s| s > 0);

    // Build a streaming parser for cost/final_message extraction.
    let mut parser = agent.output_parser();

    // Move process into spawn_blocking to drive the read loop.
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel::<Vec<u8>>(256);

    let read_handle = tokio::task::spawn_blocking(move || {
        let mut buf = [0u8; 8192];
        loop {
            match process.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let data = buf[..n].to_vec();
                    if output_tx.blocking_send(data).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("read error (may be expected at EOF): {}", e);
                    break;
                }
            }
        }
        process.wait()
    });

    // Build the timeout future.
    let timeout_fut = match timeout_secs {
        Some(secs) => tokio::time::sleep(std::time::Duration::from_secs(secs)),
        None => tokio::time::sleep(std::time::Duration::from_secs(u64::MAX / 2)),
    };
    tokio::pin!(timeout_fut);

    let mut timed_out = false;

    loop {
        tokio::select! {
            chunk = output_rx.recv() => {
                match chunk {
                    Some(data) => {
                        ctx.log_sink.write_chunk(&data).await.map_err(StepError::Io)?;
                        // Feed to streaming parser.
                        parser.parse_chunk(&data);
                        // Bounded capture for fallback raw output.
                        let remaining = max_bytes.saturating_sub(capture_buf.len());
                        if remaining > 0 {
                            let to_copy = std::cmp::min(data.len(), remaining);
                            capture_buf.extend_from_slice(&data[..to_copy]);
                        }
                    }
                    None => break, // read loop ended
                }
            }
            _ = &mut timeout_fut => {
                timed_out = true;
                if let Some(pid) = child_pid {
                    tracing::info!(
                        step_id = %step.common.id,
                        "Timeout reached, killing process tree (PID: {})", pid
                    );
                    crate::process_kill::kill_process_tree(pid).await;
                }
                break;
            }
        }
    }

    // Wait for the read handle.
    let exit_result = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        read_handle,
    )
    .await;

    let exit_code: Option<i32> = if timed_out {
        drop(output_rx);
        None
    } else {
        match exit_result {
            Ok(Ok(wait_result)) => match wait_result {
                Ok(status) => status.code(),
                Err(e) => return Err(StepError::Io(e)),
            },
            Ok(Err(_join_err)) => {
                return Err(StepError::Internal("spawn_blocking panicked".to_string()));
            }
            Err(_elapsed) => {
                if let Some(pid) = child_pid {
                    crate::process_kill::force_kill_process_tree(pid).await;
                }
                None
            }
        }
    };

    // Step 7: Write END marker.
    let _ = ctx
        .log_sink
        .write_step_end(&step.common.id, exit_code, Utc::now())
        .await
        .map_err(StepError::Io)?;

    if timed_out {
        return Err(StepError::Timeout(timeout_secs.unwrap_or(0)));
    }

    // Step 8: Finalize the parser and build output.
    let agent_output = parser.finalize();

    // Prefer the structured final_message from the parser.
    // Fall back to raw capture buffer parsed per capture spec.
    let stdout = if let Some(msg) = agent_output.final_message {
        Some(Value::String(msg))
    } else {
        let raw = parse_output(
            &capture_buf,
            step.common.capture.parser.as_deref(),
            &step.common.id,
        );
        // Only include if non-empty.
        match &raw {
            Value::String(s) if s.is_empty() => None,
            _ => Some(raw),
        }
    };

    Ok(StepOutput {
        exit_code,
        stdout,
        exports: HashMap::new(),
        cost: agent_output.cost,
    })
}

#[async_trait]
impl Step for AgentStep {
    fn kind(&self) -> &'static str {
        "agent"
    }

    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
        execute_with_spawner(self, ctx, &NoPtySpawner).await
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::models::workflow::{AgentStep, AgentType, CaptureSpec, StepDefCommon};
    use crate::pty::MockPtySpawner;
    use crate::workflow::step::{LogSink, Step, StepContext, StepOutput};

    use super::{execute_with_spawner, substitute_prompt};

    // ── Mock LogSink ──────────────────────────────────────────────────────────

    #[derive(Clone, Default)]
    struct MockLogSink {
        chunks: Arc<Mutex<Vec<u8>>>,
        events: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl LogSink for MockLogSink {
        async fn write_step_start(
            &self,
            step_id: &str,
            _started_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events
                .lock()
                .unwrap()
                .push(format!("start:{}", step_id));
            Ok(0)
        }
        async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
            self.chunks.lock().unwrap().extend_from_slice(data);
            Ok(())
        }
        async fn write_step_end(
            &self,
            step_id: &str,
            exit_code: Option<i32>,
            _finished_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events.lock().unwrap().push(format!(
                "end:{}:{}",
                step_id,
                exit_code.map(|c| c.to_string()).unwrap_or("-1".to_string())
            ));
            Ok(0)
        }
    }

    fn make_ctx(sink: Arc<dyn LogSink>) -> StepContext {
        StepContext {
            workflow_id: Uuid::now_v7(),
            workflow_version: 1,
            run_id: Uuid::now_v7(),
            step_index: 0,
            input: json!({}),
            steps: HashMap::new(),
            log_sink: sink,
            working_dir: None,
            env: HashMap::new(),
        }
    }

    fn make_agent_step(id: &str, prompt: &str) -> AgentStep {
        AgentStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            agent_type: AgentType::ClaudeCodeCli,
            prompt: prompt.to_string(),
            command_template: None,
        }
    }

    /// NDJSON that represents a single claude invocation yielding cost + final_message.
    fn make_claude_ndjson(cost: f64, duration_ms: u64, num_turns: u32, result: &str) -> Vec<u8> {
        let system_line = r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#;
        let result_line = format!(
            r#"{{"type":"result","total_cost_usd":{cost},"duration_ms":{duration_ms},"num_turns":{num_turns},"result":"{result}"}}"#,
        );
        format!("{}\n{}\n", system_line, result_line).into_bytes()
    }

    // ── Test 1: Default command template renders with literal prompt ──────────

    #[tokio::test]
    async fn test_default_command_template_with_literal_prompt() {
        let ndjson = make_claude_ndjson(0.001, 500, 1, "test answer");
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_agent_step("a1", "hello");
        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        // The mock spawner records the command used; we verify cost was captured.
        assert_eq!(output.exit_code, Some(0));
        let cost = output.cost.expect("should have cost");
        assert!((cost.total_cost_usd.unwrap() - 0.001).abs() < 1e-9);
        assert_eq!(output.stdout, Some(Value::String("test answer".to_string())));
    }

    // ── Test 2: Custom command_template with ${prompt} and ${input.*} ─────────

    #[tokio::test]
    async fn test_custom_command_template_both_substitutions() {
        let ndjson = make_claude_ndjson(0.002, 800, 2, "done");
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"session": "uuid-123"});

        let step = AgentStep {
            common: StepDefCommon {
                id: "a2".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            agent_type: AgentType::ClaudeCodeCli,
            prompt: "go".to_string(),
            command_template: Some(
                r#"claude -p "${prompt}" --resume ${input.session}"#.to_string(),
            ),
        };

        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        // The mock produces NDJSON so final_message should be "done"
        assert_eq!(output.stdout, Some(Value::String("done".to_string())));
    }

    // ── Test 3: Prompt with ${input.*} substitution ───────────────────────────

    #[tokio::test]
    async fn test_prompt_with_input_substitution() {
        // We can verify the prompt was substituted by checking that the spawner
        // would have received the resolved command. We use a mock that produces NDJSON.
        let ndjson = make_claude_ndjson(0.003, 600, 1, "reviewed acme/widgets");
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"repo": "acme/widgets"});

        let step = make_agent_step("a3", "review ${input.repo}");
        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        // The mock returns canned output so we just verify it ran without error.
        assert!(output.cost.is_some());
    }

    // ── Test 4: Mock NDJSON cost extraction ──────────────────────────────────

    #[tokio::test]
    async fn test_mock_ndjson_cost_extraction() {
        let ndjson = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.0042,"duration_ms":1500,"num_turns":3,"result":"answer"}"#,
            "\n",
        );
        let spawner =
            MockPtySpawner::with_output_and_exit(vec![ndjson.as_bytes().to_vec()], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_agent_step("a4", "compute something");
        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        let cost = output.cost.expect("should have cost");
        assert!((cost.total_cost_usd.unwrap() - 0.0042).abs() < 1e-9);
        assert_eq!(cost.duration_ms, Some(1500));
        assert_eq!(cost.num_turns, Some(3));
        assert_eq!(cost.model.as_deref(), Some("claude-opus-4-5"));
    }

    // ── Test 5: No cost data in output — cost is None ─────────────────────────

    #[tokio::test]
    async fn test_no_cost_data_cost_none() {
        // Plain shell text, no JSON
        let plain_output = b"Hello from shell\nSome output\n".to_vec();
        let spawner = MockPtySpawner::with_output_and_exit(vec![plain_output], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_agent_step("a5", "run something");
        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        assert!(
            output.cost.is_none(),
            "cost should be None when no result events in output"
        );
    }

    // ── Test 6: final_message extraction ─────────────────────────────────────

    #[tokio::test]
    async fn test_final_message_extraction() {
        let ndjson = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-5"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.001,"duration_ms":200,"num_turns":1,"result":"the answer is 42"}"#,
            "\n",
        );
        let spawner =
            MockPtySpawner::with_output_and_exit(vec![ndjson.as_bytes().to_vec()], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_agent_step("a6", "what is the answer");
        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        assert_eq!(
            output.stdout,
            Some(Value::String("the answer is 42".to_string()))
        );
    }

    // ── Unit test: substitute_prompt helper ──────────────────────────────────

    #[test]
    fn test_substitute_prompt_replaces_token() {
        let result = substitute_prompt(
            r#"claude -p "${prompt}" --verbose"#,
            "hello world",
        );
        assert_eq!(result, r#"claude -p "hello world" --verbose"#);
    }

    #[test]
    fn test_substitute_prompt_no_token_passthrough() {
        let result = substitute_prompt("echo no_prompt_here", "ignored");
        assert_eq!(result, "echo no_prompt_here");
    }

    #[test]
    fn test_substitute_prompt_dollar_in_prompt_not_re_expanded() {
        // Prompt containing ${...} should not be recursively expanded.
        let result =
            substitute_prompt(r#"claude -p "${prompt}""#, "review ${input.repo}");
        assert_eq!(result, r#"claude -p "review ${input.repo}""#);
    }

    // ── Test: kind() returns "agent" ──────────────────────────────────────────

    #[test]
    fn test_agent_step_kind() {
        let step = make_agent_step("k", "hello");
        assert_eq!(step.kind(), "agent");
    }
}
