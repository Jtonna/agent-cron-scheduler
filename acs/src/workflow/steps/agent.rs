use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use crate::pty::{NoPtySpawner, PtySpawner};
use crate::workflow::agents::impl_for;
use crate::workflow::step::{wait_for_kill, Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

pub use crate::models::workflow::AgentStep;

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

    // Step 2: Build argv via the agent runner — no shell wrapper, no escaping.
    let argv = agent.build_argv(&resolved_prompt, step.model.as_deref(), &step.extra_args);

    // Step 3: Write START marker.
    let log_byte_offset_start = ctx
        .log_sink
        .write_step_start(&step.common.id, Utc::now())
        .await
        .map_err(StepError::Io)?;
    ctx.current_step_log_offset_start = Some(log_byte_offset_start);

    // Step 4: Spawn the process via argv-array (no shell).
    let mut process = spawner
        .spawn_argv(&argv, ctx.working_dir.as_deref(), &ctx.env, 24, 80)
        .map_err(|e| StepError::Spawn(e.to_string()))?;

    // No stdin for agent steps.
    process.close_stdin();

    // Step 5: Stream output with timeout.
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
    let mut killed = false;

    let kill_rx = ctx.kill_rx.clone();

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
            _ = wait_for_kill(kill_rx.clone()) => {
                killed = true;
                if let Some(pid) = child_pid {
                    tracing::info!(
                        step_id = %step.common.id,
                        "Kill signal received, terminating process tree (PID: {})", pid
                    );
                    crate::process_kill::kill_process_tree(pid).await;
                }
                break;
            }
        }
    }

    // Wait for the read handle.
    let exit_result = tokio::time::timeout(std::time::Duration::from_secs(10), read_handle).await;

    let exit_code: Option<i32> = if timed_out || killed {
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
    let log_byte_offset_end = ctx
        .log_sink
        .write_step_end(&step.common.id, exit_code, Utc::now())
        .await
        .map_err(StepError::Io)?;
    // Record the end offset on the context so the executor can stamp it onto
    // the StepRun if this step subsequently surfaces a Killed / Timeout error
    // after the END marker has already been written.
    ctx.current_step_log_offset_end = Some(log_byte_offset_end);

    if killed {
        return Err(StepError::Killed);
    }

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
        log_byte_offset_start: Some(log_byte_offset_start),
        log_byte_offset_end: Some(log_byte_offset_end),
    })
}

#[async_trait]
impl Step for AgentStep {
    fn kind(&self) -> &'static str {
        "agent"
    }

    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
        // Use the context's spawner override when present (tests inject a
        // mock that replays a captured stream); otherwise spawn for real.
        match ctx.agent_spawner.clone() {
            Some(spawner) => execute_with_spawner(self, ctx, spawner.as_ref()).await,
            None => execute_with_spawner(self, ctx, &NoPtySpawner).await,
        }
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use indexmap::IndexMap;
    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::models::workflow::{AgentStep, AgentType, CaptureSpec, StepDefCommon};
    use crate::pty::MockPtySpawner;
    use crate::workflow::step::{LogSink, Step, StepContext};

    use super::execute_with_spawner;

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
            steps: IndexMap::new(),
            log_sink: sink,
            working_dir: None,
            env: HashMap::new(),
            event_tx: None,
            kill_rx: None,
            target_step: None,
            current_step_log_offset_start: None,
            current_step_log_offset_end: None,
            agent_spawner: None,
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
            model: None,
            extra_args: vec![],
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

    // ── Test 1: argv-array spawn with literal prompt ──────────────────────────

    #[tokio::test]
    async fn test_argv_spawn_with_literal_prompt() {
        let ndjson = make_claude_ndjson(0.001, 500, 1, "test answer");
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_agent_step("a1", "hello");
        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let cost = output.cost.expect("should have cost");
        assert!((cost.total_cost_usd.unwrap() - 0.001).abs() < 1e-9);
        assert_eq!(
            output.stdout,
            Some(Value::String("test answer".to_string()))
        );

        // Verify spawner received argv (not a shell string)
        let argv = spawner.recorded_argv().expect("argv should be recorded");
        assert_eq!(argv[0], "claude");
        assert_eq!(argv[1], "-p");
        assert_eq!(argv[2], "hello");
        assert!(argv.contains(&"--output-format".to_string()));
        assert!(argv.contains(&"stream-json".to_string()));
        assert!(argv.contains(&"--dangerously-skip-permissions".to_string()));
    }

    // ── Test 2: argv with extra_args (replaces custom command_template) ────────

    #[tokio::test]
    async fn test_argv_spawn_with_extra_args() {
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
            model: None,
            extra_args: vec!["--resume".to_string(), "uuid-123".to_string()],
        };

        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        assert_eq!(output.stdout, Some(Value::String("done".to_string())));

        // Verify extra_args appear at the end of argv
        let argv = spawner.recorded_argv().expect("argv should be recorded");
        let last_two: Vec<&str> = argv.iter().rev().take(2).map(|s| s.as_str()).collect();
        assert_eq!(last_two[0], "uuid-123");
        assert_eq!(last_two[1], "--resume");
    }

    // ── Test 3: Prompt with ${input.*} substitution ───────────────────────────

    #[tokio::test]
    async fn test_prompt_with_input_substitution() {
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
        assert!(output.cost.is_some());

        // Verify the prompt was substituted and passed as a discrete argv element
        let argv = spawner.recorded_argv().expect("argv should be recorded");
        assert_eq!(argv[2], "review acme/widgets");
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
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson.as_bytes().to_vec()], 0);

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
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson.as_bytes().to_vec()], 0);

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

    // ── Test 7: argv with model override ─────────────────────────────────────

    #[tokio::test]
    async fn test_argv_spawn_with_model_override() {
        let ndjson = make_claude_ndjson(0.005, 1000, 2, "model answer");
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = AgentStep {
            common: StepDefCommon {
                id: "a7".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            agent_type: AgentType::ClaudeCodeCli,
            prompt: "test".to_string(),
            model: Some("claude-opus-4-5".to_string()),
            extra_args: vec![],
        };

        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));

        let argv = spawner.recorded_argv().expect("argv should be recorded");
        let model_pos = argv
            .iter()
            .position(|a| a == "--model")
            .expect("--model should be present");
        assert_eq!(argv[model_pos + 1], "claude-opus-4-5");
    }

    // ── Test 8: Prompt with special chars — no shell escaping needed ──────────

    #[tokio::test]
    async fn test_prompt_with_special_chars_no_escaping_needed() {
        let ndjson = make_claude_ndjson(0.001, 300, 1, "ok");
        let spawner = MockPtySpawner::with_output_and_exit(vec![ndjson], 0);

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let tricky_prompt = "Review: \"foo, bar?\" & newline\nsecond line";
        let step = make_agent_step("a8", tricky_prompt);

        let output = execute_with_spawner(&step, &mut ctx, &spawner)
            .await
            .expect("execute should succeed");
        assert_eq!(output.exit_code, Some(0));

        // argv-array spawn: the prompt is element [2], verbatim, no escaping.
        let argv = spawner.recorded_argv().expect("argv should be recorded");
        assert_eq!(argv[2], tricky_prompt);
        assert!(argv[2].contains('"'));
        assert!(argv[2].contains('\n'));
    }

    // ── Test: kind() returns "agent" ──────────────────────────────────────────

    #[test]
    fn test_agent_step_kind() {
        let step = make_agent_step("k", "hello");
        assert_eq!(step.kind(), "agent");
    }
}
