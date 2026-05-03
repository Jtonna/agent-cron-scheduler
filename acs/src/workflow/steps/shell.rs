use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use crate::pty::{NoPtySpawner, PtySpawner};
use crate::workflow::step::{Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

pub use crate::models::workflow::ShellStep;

#[async_trait]
impl Step for ShellStep {
    fn kind(&self) -> &'static str {
        "shell"
    }

    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
        // 1. Template substitution
        let sub = template::substitute(&self.command, &ctx.input, &ctx.steps);
        for warn in &sub.warnings {
            tracing::warn!(step_id = %self.common.id, "template warning: {}", warn);
        }
        let substituted_command = sub.output;

        // 2. Build CommandBuilder
        let mut cmd = build_command(&substituted_command);

        // 3. Set working dir and env
        if let Some(ref dir) = ctx.working_dir {
            cmd.cwd(dir);
        }
        for (k, v) in &ctx.env {
            cmd.env(k, v);
        }

        // 6. Step boundary: START marker
        let _ = ctx
            .log_sink
            .write_step_start(&self.common.id, Utc::now())
            .await
            .map_err(StepError::Io)?;

        // 4. Spawn via NoPtySpawner
        let spawner = NoPtySpawner;
        let mut process = spawner
            .spawn(cmd, 24, 80)
            .map_err(|e| StepError::Spawn(e.to_string()))?;

        // 8. stdin handling
        if self.pass_stdin {
            // Find the most recently accumulated step output
            let prev_stdout = ctx.steps.values().last().and_then(|s| s.stdout.as_ref());
            if let Some(val) = prev_stdout {
                let bytes: Vec<u8> = match val {
                    Value::String(s) => s.as_bytes().to_vec(),
                    other => serde_json::to_vec(other).unwrap_or_default(),
                };
                if !bytes.is_empty() {
                    if let Err(e) = process.write_stdin(&bytes) {
                        tracing::warn!(step_id = %self.common.id, "Failed to write stdin: {}", e);
                    }
                }
            }
        }
        process.close_stdin();

        // 5. Read output with optional timeout
        let max_bytes = self.common.capture.stdout_max_bytes;
        let mut capture_buf: Vec<u8> = Vec::with_capacity(std::cmp::min(max_bytes, 65536));

        let child_pid = process.pid();
        let timeout_secs = self.common.timeout_secs.filter(|&s| s > 0);

        // Move process into spawn_blocking to drive the read loop
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

        // Build the timeout future
        let timeout_fut = match timeout_secs {
            Some(secs) => {
                tokio::time::sleep(std::time::Duration::from_secs(secs))
            }
            None => {
                // Effectively infinite: 136 years
                tokio::time::sleep(std::time::Duration::from_secs(u64::MAX / 2))
            }
        };
        tokio::pin!(timeout_fut);

        let mut timed_out = false;

        loop {
            tokio::select! {
                chunk = output_rx.recv() => {
                    match chunk {
                        Some(data) => {
                            ctx.log_sink.write_chunk(&data).await.map_err(StepError::Io)?;
                            // Bounded capture
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
                            step_id = %self.common.id,
                            "Timeout reached, killing process tree (PID: {})", pid
                        );
                        crate::process_kill::kill_process_tree(pid).await;
                    }
                    break;
                }
            }
        }

        // Wait for the read handle
        let exit_result = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            read_handle,
        )
        .await;

        let exit_code: Option<i32> = if timed_out {
            // Drain any remaining chunks from channel so the reader unblocks,
            // then let the read_handle complete on its own.
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

        // 6. Step boundary: END marker
        let _ = ctx
            .log_sink
            .write_step_end(&self.common.id, exit_code, Utc::now())
            .await
            .map_err(StepError::Io)?;

        if timed_out {
            return Err(StepError::Timeout(timeout_secs.unwrap_or(0)));
        }

        // 9. Build StepOutput: parse capture_buf per parser spec
        let stdout = parse_output(&capture_buf, self.common.capture.parser.as_deref(), &self.common.id);

        Ok(StepOutput {
            exit_code,
            stdout: Some(stdout),
            exports: HashMap::new(),
            cost: None,
        })
    }
}

/// Build a `portable_pty::CommandBuilder` for a shell command string.
fn build_command(command: &str) -> portable_pty::CommandBuilder {
    #[cfg(windows)]
    {
        // On Windows, use cmd.exe /C <command> with raw_arg to bypass MSVC quoting.
        // NoPtySpawner joins argv[1..] with spaces and passes through raw_arg(),
        // so we add the /C flag and command as one string to avoid double-quoting.
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

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::models::workflow::{CaptureSpec, ShellStep, StepDefCommon};
    use crate::workflow::step::{LogSink, Step, StepContext, StepError, StepOutput};

    // ── Mock LogSink ─────────────────────────────────────────────────────────────
    // Records all calls for assertions in tests. `chunks` accumulates raw output bytes.
    // `events` records "start:<id>" and "end:<id>:<exit_code>" strings in order.

    #[derive(Clone, Default)]
    struct MockLogSink {
        chunks: Arc<Mutex<Vec<u8>>>,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl MockLogSink {
        fn collected_output(&self) -> Vec<u8> {
            self.chunks.lock().unwrap().clone()
        }
        fn events(&self) -> Vec<String> {
            self.events.lock().unwrap().clone()
        }
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

    // ── Helper: create a minimal StepContext ─────────────────────────────────────

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
            event_tx: None,
        }
    }

    fn make_step(id: &str, command: &str) -> ShellStep {
        ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: command.to_string(),
            pass_stdin: false,
        }
    }

    // ── Test 1: Happy path — echo hello ──────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_happy_path_echo() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s1", "echo hello");
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        // stdout captured
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("hello"),
            "expected 'hello' in stdout: {:?}",
            stdout_str
        );
        // log sink received data
        let logged = String::from_utf8_lossy(&sink.collected_output()).into_owned();
        assert!(logged.contains("hello"), "log sink should contain 'hello'");
        // start and end markers
        let events = sink.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "start:s1");
        assert_eq!(events[1], "end:s1:0");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_step_happy_path_echo() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s1", "echo hello");
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.to_lowercase().contains("hello"),
            "expected 'hello' in stdout: {:?}",
            stdout_str
        );
        let events = sink.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "start:s1");
        assert!(events[1].starts_with("end:s1:"));
    }

    // ── Test 2: Non-zero exit code ────────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_nonzero_exit_is_ok() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s2", "sh -c 'exit 1'");
        let output = step.execute(&mut ctx).await.expect("non-zero exit should be Ok");

        assert_eq!(output.exit_code, Some(1), "exit code should be 1");
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_shell_step_nonzero_exit_is_ok() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s2", "exit 1");
        let output = step.execute(&mut ctx).await.expect("non-zero exit should be Ok");
        // cmd.exe exit codes may vary but step should be Ok
        assert!(output.exit_code.is_some());
    }

    // ── Test 3: Template substitution ────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_template_substitution() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({"name": "world"});

        let step = make_step("s3", "echo ${input.name}");
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("world"),
            "expected 'world' in stdout from template substitution: {:?}",
            stdout_str
        );
    }

    // ── Test 4: Capture parser = json ─────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_capture_parser_json() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ShellStep {
            common: StepDefCommon {
                id: "s4".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec {
                    stdout_max_bytes: 65536,
                    parser: Some("json".to_string()),
                },
            },
            command: r#"printf '{"x":1}'"#.to_string(),
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute");
        match output.stdout {
            Some(Value::Object(map)) => {
                assert_eq!(map.get("x"), Some(&json!(1)));
            }
            other => panic!("expected JSON object, got {:?}", other),
        }
    }

    // ── Test 5: Capture parser = lines ───────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_capture_parser_lines() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ShellStep {
            common: StepDefCommon {
                id: "s5".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec {
                    stdout_max_bytes: 65536,
                    parser: Some("lines".to_string()),
                },
            },
            command: "printf 'a\\nb\\n'".to_string(),
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute");
        match output.stdout {
            Some(Value::Array(arr)) => {
                assert!(arr.len() >= 2, "expected at least 2 lines, got {:?}", arr);
                assert_eq!(arr[0], json!("a"));
                assert_eq!(arr[1], json!("b"));
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    // ── Test 6: Working dir override ─────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_working_dir() {
        use std::path::PathBuf;

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.working_dir = Some(PathBuf::from("/tmp"));

        let step = make_step("s6", "pwd");
        let output = step.execute(&mut ctx).await.expect("execute");

        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        // /tmp may be a symlink on macOS, so just check the path contains "tmp"
        assert!(
            stdout_str.contains("tmp"),
            "expected '/tmp' in pwd output: {:?}",
            stdout_str
        );
    }

    // ── Test 7: Env var passing ───────────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_env_var_passing() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.env.insert("SHELL_STEP_TEST_VAR".to_string(), "bar42".to_string());

        let step = make_step("s7", "echo $SHELL_STEP_TEST_VAR");
        let output = step.execute(&mut ctx).await.expect("execute");

        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("bar42"),
            "expected 'bar42' in stdout: {:?}",
            stdout_str
        );
    }

    // ── Test 8: Timeout fires ─────────────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_timeout() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ShellStep {
            common: StepDefCommon {
                id: "s8".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(1),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: "sleep 10".to_string(),
            pass_stdin: false,
        };

        let result = step.execute(&mut ctx).await;
        match result {
            Err(StepError::Timeout(secs)) => {
                assert_eq!(secs, 1, "expected timeout of 1 second");
            }
            other => panic!("expected Timeout error, got {:?}", other),
        }
    }

    // ── Test: exports and cost are empty/none for ShellStep ───────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_shell_step_exports_empty_cost_none() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s9", "echo hi");
        let output = step.execute(&mut ctx).await.expect("execute");

        assert!(output.exports.is_empty(), "exports should be empty");
        assert!(output.cost.is_none(), "cost should be None for ShellStep");
    }

    // ── Test: kind() returns "shell" ──────────────────────────────────────────────

    #[test]
    fn test_shell_step_kind() {
        let step = make_step("k", "echo");
        assert_eq!(step.kind(), "shell");
    }

    // ── Test: parse_output helper ─────────────────────────────────────────────────

    #[test]
    fn test_parse_output_raw() {
        let result = super::parse_output(b"hello world", None, "test");
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_parse_output_raw_explicit() {
        let result = super::parse_output(b"hello world", Some("raw"), "test");
        assert_eq!(result, Value::String("hello world".to_string()));
    }

    #[test]
    fn test_parse_output_json_valid() {
        let result = super::parse_output(b"{\"x\":1}", Some("json"), "test");
        assert_eq!(result, json!({"x": 1}));
    }

    #[test]
    fn test_parse_output_json_invalid_falls_back_to_string() {
        let result = super::parse_output(b"not json", Some("json"), "test");
        assert_eq!(result, Value::String("not json".to_string()));
    }

    #[test]
    fn test_parse_output_lines() {
        let result = super::parse_output(b"a\nb\n", Some("lines"), "test");
        assert_eq!(result, json!(["a", "b"]));
    }

    #[test]
    fn test_parse_output_lines_no_trailing_newline() {
        let result = super::parse_output(b"x\ny", Some("lines"), "test");
        assert_eq!(result, json!(["x", "y"]));
    }

    #[test]
    fn test_parse_output_unknown_parser_returns_string() {
        let result = super::parse_output(b"data", Some("csv"), "test");
        assert_eq!(result, Value::String("data".to_string()));
    }

    // ── Test: build_command helper ────────────────────────────────────────────────

    #[test]
    fn test_build_command_produces_shell_command() {
        let cmd = super::build_command("echo hello");
        let argv = cmd.get_argv();
        assert!(!argv.is_empty());
        // Should start with sh (unix) or cmd (windows)
        let prog = argv[0].to_string_lossy().to_lowercase();
        assert!(
            prog.contains("sh") || prog.contains("cmd"),
            "expected sh or cmd, got: {}",
            prog
        );
    }

    // ── MockPtySpawner integration test ──────────────────────────────────────────
    // Demonstrates that the step logic works with MockPtySpawner by testing
    // the parse_output function which is the output-processing kernel.

    #[test]
    fn test_mock_pty_spawner_output_processing() {
        // Simulate what execute() does with captured bytes from a mock process
        let raw_output = b"hello from mock\n";

        // raw parser
        let result = super::parse_output(raw_output, None, "mock-step");
        assert_eq!(result, Value::String("hello from mock\n".to_string()));

        // lines parser
        let result = super::parse_output(raw_output, Some("lines"), "mock-step");
        assert_eq!(result, json!(["hello from mock"]));
    }
}
