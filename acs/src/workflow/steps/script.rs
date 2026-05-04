use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde_json::Value;

use crate::pty::{NoPtySpawner, PtySpawner};
use crate::workflow::step::{wait_for_kill, Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

pub use crate::models::workflow::ScriptStep;

#[async_trait]
impl Step for ScriptStep {
    fn kind(&self) -> &'static str {
        "script"
    }

    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
        // 1. Template substitution for path
        let path_sub = template::substitute(&self.path, &ctx.input, &ctx.steps);
        for warn in &path_sub.warnings {
            tracing::warn!(step_id = %self.common.id, "template warning (path): {}", warn);
        }
        let substituted_path = path_sub.output;

        // 1. Template substitution for args (if present)
        let substituted_args: Option<String> = if let Some(ref args) = self.args {
            let args_sub = template::substitute(args, &ctx.input, &ctx.steps);
            for warn in &args_sub.warnings {
                tracing::warn!(step_id = %self.common.id, "template warning (args): {}", warn);
            }
            Some(args_sub.output)
        } else {
            None
        };

        // 2. Determine interpreter based on script_type
        let mut cmd = build_script_command(
            self.script_type.as_deref(),
            &substituted_path,
            substituted_args.as_deref(),
        )?;

        // 3. Set working dir and env
        if let Some(ref dir) = ctx.working_dir {
            cmd.cwd(dir);
        }
        for (k, v) in &ctx.env {
            cmd.env(k, v);
        }

        // 5. Step boundary: START marker
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

        // 6. Stream output with optional timeout + bounded capture buffer
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
            Some(secs) => tokio::time::sleep(std::time::Duration::from_secs(secs)),
            None => {
                // Effectively infinite: 136 years
                tokio::time::sleep(std::time::Duration::from_secs(u64::MAX / 2))
            }
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
                _ = wait_for_kill(kill_rx.clone()) => {
                    killed = true;
                    if let Some(pid) = child_pid {
                        tracing::info!(
                            step_id = %self.common.id,
                            "Kill signal received, terminating process tree (PID: {})", pid
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

        let exit_code: Option<i32> = if timed_out || killed {
            // Drain remaining chunks so reader unblocks
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

        // 5. Step boundary: END marker
        let _ = ctx
            .log_sink
            .write_step_end(&self.common.id, exit_code, Utc::now())
            .await
            .map_err(StepError::Io)?;

        if killed {
            return Err(StepError::Killed);
        }

        if timed_out {
            return Err(StepError::Timeout(timeout_secs.unwrap_or(0)));
        }

        // 9. Build StepOutput: parse capture_buf per parser spec
        let stdout = parse_output(
            &capture_buf,
            self.common.capture.parser.as_deref(),
            &self.common.id,
        );

        Ok(StepOutput {
            exit_code,
            stdout: Some(stdout),
            exports: HashMap::new(),
            cost: None,
        })
    }
}

/// Check whether a command is available on the system PATH.
///
/// On Windows uses `where`; on Unix uses `which`. Returns `false` on error.
fn is_command_available(cmd: &str) -> bool {
    #[cfg(windows)]
    let check = std::process::Command::new("where")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    #[cfg(not(windows))]
    let check = std::process::Command::new("which")
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    check.map(|s| s.success()).unwrap_or(false)
}

/// Build a `portable_pty::CommandBuilder` for a script file with the given interpreter.
fn build_script_command(
    script_type: Option<&str>,
    path: &str,
    args: Option<&str>,
) -> Result<portable_pty::CommandBuilder, StepError> {
    let mut cmd = match script_type {
        Some("shell") | None => {
            // On Unix: sh <path>; on Windows: cmd /C <path>
            #[cfg(windows)]
            {
                let mut c = portable_pty::CommandBuilder::new("cmd");
                // Pass /C + path as a single raw arg to avoid quoting issues
                let arg = if let Some(a) = args {
                    format!("/C {} {}", path, a)
                } else {
                    format!("/C {}", path)
                };
                c.arg(arg);
                return Ok(c);
            }
            #[cfg(not(windows))]
            {
                let mut c = portable_pty::CommandBuilder::new("sh");
                c.arg(path);
                if let Some(a) = args {
                    // Split whitespace-separated args
                    for part in a.split_whitespace() {
                        c.arg(part);
                    }
                }
                return Ok(c);
            }
        }
        Some("batch") => {
            #[cfg(windows)]
            {
                let mut c = portable_pty::CommandBuilder::new("cmd");
                let arg = if let Some(a) = args {
                    format!("/C {} {}", path, a)
                } else {
                    format!("/C {}", path)
                };
                c.arg(arg);
                return Ok(c);
            }
            #[cfg(not(windows))]
            {
                return Err(StepError::Internal(
                    "script_type 'batch' is only supported on Windows".to_string(),
                ));
            }
        }
        Some("python") => {
            #[cfg(windows)]
            let interpreter = "python";
            #[cfg(not(windows))]
            let interpreter = "python3";

            let mut c = portable_pty::CommandBuilder::new(interpreter);
            c.arg(path);
            if let Some(a) = args {
                for part in a.split_whitespace() {
                    c.arg(part);
                }
            }
            c
        }
        Some("powershell") => {
            // On Windows: prefer pwsh (PowerShell 7+ Core); fall back to
            // powershell.exe (Windows PowerShell 5.1) when pwsh is not on PATH.
            // On Unix: always use pwsh.
            #[cfg(windows)]
            let interpreter = if is_command_available("pwsh") {
                "pwsh"
            } else {
                "powershell.exe"
            };
            #[cfg(not(windows))]
            let interpreter = "pwsh";

            let mut c = portable_pty::CommandBuilder::new(interpreter);
            c.arg("-File");
            c.arg(path);
            if let Some(a) = args {
                for part in a.split_whitespace() {
                    c.arg(part);
                }
            }
            c
        }
        Some(other) => {
            return Err(StepError::Internal(format!(
                "unknown script_type: {}",
                other
            )));
        }
    };

    // If we reach here without returning, add args (this handles the
    // python/powershell cases that fall through above — but the explicit
    // returns above already handle them, so this is a safety net).
    if let Some(a) = args {
        for part in a.split_whitespace() {
            cmd.arg(part);
        }
    }

    Ok(cmd)
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
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use indexmap::IndexMap;
    use serde_json::{json, Value};
    use tempfile::TempDir;
    use uuid::Uuid;

    use crate::models::workflow::{CaptureSpec, ScriptStep, StepDefCommon};
    use crate::workflow::step::{LogSink, Step, StepContext, StepError};

    // ── Mock LogSink ─────────────────────────────────────────────────────────────

    #[derive(Clone, Default)]
    struct MockLogSink {
        chunks: Arc<Mutex<Vec<u8>>>,
        events: Arc<Mutex<Vec<String>>>,
    }

    #[allow(dead_code)]
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
                exit_code
                    .map(|c| c.to_string())
                    .unwrap_or_else(|| "-1".to_string())
            ));
            Ok(0)
        }
    }

    // ── Helpers ──────────────────────────────────────────────────────────────────

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
        }
    }

    fn make_step(id: &str, path: &str, script_type: Option<&str>) -> ScriptStep {
        ScriptStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            path: path.to_string(),
            script_type: script_type.map(|s| s.to_string()),
            args: None,
            pass_stdin: false,
        }
    }

    /// Check if a command is available on PATH.
    fn has_command(cmd: &str) -> bool {
        #[cfg(windows)]
        let check = std::process::Command::new("where")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        #[cfg(not(windows))]
        let check = std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();

        check.map(|s| s.success()).unwrap_or(false)
    }

    /// Write a file into a TempDir and make it executable on Unix.
    fn write_script(dir: &TempDir, filename: &str, content: &str) -> std::path::PathBuf {
        let path = dir.path().join(filename);
        let mut f = std::fs::File::create(&path).expect("create script");
        f.write_all(content.as_bytes()).expect("write script");
        drop(f);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("set permissions");
        }

        path
    }

    // ── Test 1: Shell script (Unix only) ────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_shell_unix() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "hello.sh", "#!/bin/sh\necho hello\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s1", path.to_str().unwrap(), Some("shell"));
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("hello"),
            "expected 'hello' in stdout: {:?}",
            stdout_str
        );

        let events = sink.events();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "start:s1");
        assert_eq!(events[1], "end:s1:0");
    }

    // ── Test 2: Batch script (Windows only) ────────────────────────────────────

    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_batch_windows() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "hello.bat", "@echo off\necho hello\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s2", path.to_str().unwrap(), Some("batch"));
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
    }

    // ── Test 3: Python script (cross-platform) ──────────────────────────────────

    #[tokio::test]
    async fn test_script_step_python() {
        #[cfg(windows)]
        let python_bin = "python";
        #[cfg(not(windows))]
        let python_bin = "python3";

        if !has_command(python_bin) {
            eprintln!("Skipping test_script_step_python: {} not found", python_bin);
            return;
        }

        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "hello.py", "print('hi')\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s3", path.to_str().unwrap(), Some("python"));
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("hi"),
            "expected 'hi' in stdout: {:?}",
            stdout_str
        );
    }

    // ── Test 4: PowerShell script (cross-platform) ──────────────────────────────

    #[tokio::test]
    async fn test_script_step_powershell() {
        if !has_command("pwsh") {
            eprintln!("Skipping test_script_step_powershell: pwsh not found");
            return;
        }

        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "hello.ps1", "Write-Host 'hi'\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s4", path.to_str().unwrap(), Some("powershell"));
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("hi"),
            "expected 'hi' in stdout: {:?}",
            stdout_str
        );
    }

    // ── Test 5: Unknown script_type ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_script_step_unknown_script_type() {
        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s5", "/some/script.rs", Some("rust"));
        let result = step.execute(&mut ctx).await;

        match result {
            Err(StepError::Internal(msg)) => {
                assert!(
                    msg.contains("unknown script_type"),
                    "expected 'unknown script_type' in error: {:?}",
                    msg
                );
            }
            other => panic!("expected StepError::Internal, got {:?}", other),
        }
    }

    // ── Test 6: Templated path and args ─────────────────────────────────────────
    // Unix: sh script receives $1. Windows: batch script receives %1.
    // Both test that template substitution resolves ${input.*} in both path and args.

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_templated_path_and_args() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "greet.sh", "#!/bin/sh\necho \"greet:$1\"\n");
        let dir_str = dir.path().to_str().unwrap().to_string();

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({
            "script_dir": dir_str,
            "arg": "world"
        });

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s6".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            path: "${input.script_dir}/greet.sh".to_string(),
            script_type: Some("shell".to_string()),
            args: Some("${input.arg}".to_string()),
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("greet:world"),
            "expected 'greet:world' in stdout: {:?}",
            stdout_str
        );
    }

    // Windows equivalent: batch script using %1 for the first argument.
    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_templated_path_and_args() {
        let dir = TempDir::new().expect("tmpdir");
        let _path = write_script(&dir, "greet.bat", "@echo off\necho greet:%1\n");
        let dir_str = dir.path().to_str().unwrap().to_string();

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);
        ctx.input = json!({
            "script_dir": dir_str,
            "arg": "world"
        });

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s6".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            path: "${input.script_dir}\\greet.bat".to_string(),
            script_type: Some("batch".to_string()),
            args: Some("${input.arg}".to_string()),
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.to_lowercase().contains("greet:world"),
            "expected 'greet:world' in stdout: {:?}",
            stdout_str
        );
    }

    // ── Test 7: No script_type fallback ─────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_no_script_type_fallback_unix() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "hello.sh", "#!/bin/sh\necho hello_fallback\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s7", path.to_str().unwrap(), None);
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.contains("hello_fallback"),
            "expected 'hello_fallback' in stdout: {:?}",
            stdout_str
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_no_script_type_fallback_windows() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "hello.bat", "@echo off\necho hello_fallback\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = make_step("s7", path.to_str().unwrap(), None);
        let output = step.execute(&mut ctx).await.expect("execute should succeed");

        assert_eq!(output.exit_code, Some(0));
        let stdout_str = match output.stdout {
            Some(Value::String(s)) => s,
            other => panic!("expected String, got {:?}", other),
        };
        assert!(
            stdout_str.to_lowercase().contains("hello_fallback"),
            "expected 'hello_fallback' in stdout: {:?}",
            stdout_str
        );
    }

    // ── Test 8: Capture parser variants ─────────────────────────────────────────
    // Unix: sh scripts using printf for precise output.
    // Windows: batch scripts using echo (with trailing space/CR awareness).

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_capture_parser_json() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(
            &dir,
            "json.sh",
            "#!/bin/sh\nprintf '{\"x\":1}'\n",
        );

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s8a".to_string(),
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
            path: path.to_str().unwrap().to_string(),
            script_type: Some("shell".to_string()),
            args: None,
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

    // Windows: use a powershell.exe call inside a batch script for clean JSON output
    // (cmd.exe `echo` adds a trailing space before \r\n which breaks JSON parsing).
    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_capture_parser_json() {
        let dir = TempDir::new().expect("tmpdir");
        // Use powershell.exe (always available on Windows) to emit clean JSON.
        let path = write_script(
            &dir,
            "json.bat",
            "@echo off\npowershell -NoProfile -Command \"Write-Output '{\\\"x\\\":1}'\"\n",
        );

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s8a".to_string(),
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
            path: path.to_str().unwrap().to_string(),
            script_type: Some("batch".to_string()),
            args: None,
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

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_capture_parser_lines() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(
            &dir,
            "lines.sh",
            "#!/bin/sh\nprintf 'a\\nb\\n'\n",
        );

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s8b".to_string(),
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
            path: path.to_str().unwrap().to_string(),
            script_type: Some("shell".to_string()),
            args: None,
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

    // Windows: two echo lines from a batch script.
    // cmd.exe echo includes trailing space + \r\n; the lines parser splits on '\n'
    // leaving '\r' in the value — we just check containment.
    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_capture_parser_lines() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(
            &dir,
            "lines.bat",
            "@echo off\necho lineA\necho lineB\n",
        );

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s8b".to_string(),
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
            path: path.to_str().unwrap().to_string(),
            script_type: Some("batch".to_string()),
            args: None,
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute");
        match output.stdout {
            Some(Value::Array(arr)) => {
                assert!(arr.len() >= 2, "expected at least 2 lines, got {:?}", arr);
                let first = arr[0].as_str().unwrap_or("").trim().to_lowercase();
                let second = arr[1].as_str().unwrap_or("").trim().to_lowercase();
                assert!(
                    first.contains("linea"),
                    "expected 'lineA' in first line: {:?}",
                    arr[0]
                );
                assert!(
                    second.contains("lineb"),
                    "expected 'lineB' in second line: {:?}",
                    arr[1]
                );
            }
            other => panic!("expected Array, got {:?}", other),
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_capture_parser_raw() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "raw.sh", "#!/bin/sh\necho rawdata\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s8c".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec {
                    stdout_max_bytes: 65536,
                    parser: Some("raw".to_string()),
                },
            },
            path: path.to_str().unwrap().to_string(),
            script_type: Some("shell".to_string()),
            args: None,
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute");
        match output.stdout {
            Some(Value::String(s)) => {
                assert!(s.contains("rawdata"), "expected 'rawdata' in raw output: {:?}", s);
            }
            other => panic!("expected String, got {:?}", other),
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_capture_parser_raw() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "raw.bat", "@echo off\necho rawdata\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s8c".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec {
                    stdout_max_bytes: 65536,
                    parser: Some("raw".to_string()),
                },
            },
            path: path.to_str().unwrap().to_string(),
            script_type: Some("batch".to_string()),
            args: None,
            pass_stdin: false,
        };

        let output = step.execute(&mut ctx).await.expect("execute");
        match output.stdout {
            Some(Value::String(s)) => {
                assert!(
                    s.to_lowercase().contains("rawdata"),
                    "expected 'rawdata' in raw output: {:?}",
                    s
                );
            }
            other => panic!("expected String, got {:?}", other),
        }
    }

    // ── Test 9: Timeout ──────────────────────────────────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_script_step_timeout() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(&dir, "slow.sh", "#!/bin/sh\nsleep 10\n");

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s9".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(1),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            path: path.to_str().unwrap().to_string(),
            script_type: Some("shell".to_string()),
            args: None,
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

    // Windows: batch script that sleeps via PowerShell Start-Sleep.
    // We avoid using `timeout.exe` here because it may be shadowed by GNU coreutils
    // on developer machines (e.g. Git for Windows adds it to PATH).
    #[cfg(windows)]
    #[tokio::test]
    async fn test_script_step_timeout() {
        let dir = TempDir::new().expect("tmpdir");
        let path = write_script(
            &dir,
            "slow.bat",
            "@echo off\npowershell -NoProfile -Command \"Start-Sleep -Seconds 10\"\n",
        );

        let sink = Arc::new(MockLogSink::default());
        let mut ctx = make_ctx(Arc::clone(&sink) as Arc<dyn LogSink>);

        let step = ScriptStep {
            common: StepDefCommon {
                id: "s9".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(1),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            path: path.to_str().unwrap().to_string(),
            script_type: Some("batch".to_string()),
            args: None,
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

    // ── Unit tests: kind() and parse_output ──────────────────────────────────────

    #[test]
    fn test_script_step_kind() {
        let step = make_step("k", "/some/script.sh", Some("shell"));
        assert_eq!(step.kind(), "script");
    }

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
    fn test_parse_output_unknown_parser_returns_string() {
        let result = super::parse_output(b"data", Some("csv"), "test");
        assert_eq!(result, Value::String("data".to_string()));
    }

    // ── Unit test: build_script_command ──────────────────────────────────────────

    #[test]
    fn test_build_script_command_unknown_type_errors() {
        let result = super::build_script_command(Some("rust"), "/some/path.rs", None);
        assert!(result.is_err());
        let err = result.err().unwrap();
        assert!(
            matches!(err, StepError::Internal(ref msg) if msg.contains("unknown script_type")),
            "expected Internal error, got: {:?}",
            err
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_build_script_command_shell_windows() {
        let result = super::build_script_command(Some("shell"), "C:\\scripts\\run.sh", None);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        let argv = cmd.get_argv();
        assert!(!argv.is_empty());
        let prog = argv[0].to_string_lossy().to_lowercase();
        assert!(prog.contains("cmd"), "expected cmd, got: {}", prog);
    }

    #[cfg(not(windows))]
    #[test]
    fn test_build_script_command_shell_unix() {
        let result = super::build_script_command(Some("shell"), "/tmp/run.sh", None);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        let argv = cmd.get_argv();
        assert!(!argv.is_empty());
        let prog = argv[0].to_string_lossy();
        assert_eq!(prog, "sh");
    }

    #[cfg(not(windows))]
    #[test]
    fn test_build_script_command_batch_errors_on_unix() {
        let result = super::build_script_command(Some("batch"), "/tmp/run.bat", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_build_script_command_python() {
        let result = super::build_script_command(Some("python"), "/tmp/run.py", None);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        let argv = cmd.get_argv();
        assert!(!argv.is_empty());
        let prog = argv[0].to_string_lossy();
        assert!(
            prog.contains("python"),
            "expected python interpreter, got: {}",
            prog
        );
    }

    // On non-Windows: pwsh is always selected.
    #[cfg(not(windows))]
    #[test]
    fn test_build_script_command_powershell_unix_always_pwsh() {
        let result = super::build_script_command(Some("powershell"), "/tmp/run.ps1", None);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        let argv = cmd.get_argv();
        assert!(!argv.is_empty());
        let prog = argv[0].to_string_lossy();
        assert_eq!(prog, "pwsh", "on Unix pwsh must always be selected");
    }

    // On Windows: selected interpreter must match what's available on PATH.
    #[cfg(windows)]
    #[test]
    fn test_build_script_command_powershell_windows_correct_interpreter() {
        let result = super::build_script_command(Some("powershell"), "C:\\scripts\\run.ps1", None);
        assert!(result.is_ok());
        let cmd = result.unwrap();
        let argv = cmd.get_argv();
        assert!(!argv.is_empty());
        let prog = argv[0].to_string_lossy().to_lowercase();

        let pwsh_available = has_command("pwsh");
        if pwsh_available {
            assert_eq!(
                prog, "pwsh",
                "when pwsh is available it must be preferred over powershell.exe"
            );
        } else {
            assert!(
                prog.contains("powershell"),
                "when pwsh is absent, powershell.exe must be used; got: {}",
                prog
            );
        }
    }

    // On Windows: verify is_command_available works for a guaranteed command.
    #[cfg(windows)]
    #[test]
    fn test_is_command_available_cmd_always_found() {
        assert!(
            super::is_command_available("cmd"),
            "cmd.exe should always be detectable on Windows"
        );
        assert!(
            !super::is_command_available("__acs_fake_command_xyz_not_exist__"),
            "fake command should not be detected as available"
        );
    }

    #[test]
    fn test_build_script_command_none_type() {
        let result = super::build_script_command(None, "/tmp/run.sh", None);
        assert!(result.is_ok());
    }
}
