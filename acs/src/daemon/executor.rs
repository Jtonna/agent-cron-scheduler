use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing;
use uuid::Uuid;

use crate::daemon::events::JobEvent;
use crate::models::TriggerParams;
use crate::models::{DaemonConfig, ExecutionType, Job, JobRun, RunStatus};
use crate::pty::PtySpawner;
use crate::storage::LogStore;

/// Result of executing a hook command.
enum HookOutcome {
    /// Hook succeeded (exit code zero) with captured stdout.
    Success(Vec<u8>),
    /// Hook failed: carries a human-readable description of the failure and any captured stdout.
    Failure(String, Vec<u8>),
}

/// Execute a hook command string using the platform shell.
///
/// * `command`     – the shell command to run
/// * `working_dir` – optional working directory (inherits cwd if None)
/// * `env_vars`    – optional extra environment variables to set
/// * `extra_env`   – optional single additional key/value env pair (e.g. ACS_JOB_EXIT_CODE)
/// * `label`       – a short label used in log/error messages ("pre-hook" or "post-hook")
///
/// Returns [`HookOutcome::Success`] when the process exits with code 0, or
/// [`HookOutcome::Failure`] for any other outcome (non-zero exit, timeout,
/// spawn error, etc.).
async fn run_hook(
    command: &str,
    working_dir: Option<&str>,
    env_vars: Option<&HashMap<String, String>>,
    extra_env: Option<(&str, &str)>,
    label: &str,
) -> HookOutcome {
    let mut cmd = {
        #[cfg(target_os = "windows")]
        {
            // On Windows, cmd.exe /C needs the command string passed without
            // Rust's automatic re-quoting, otherwise embedded quotes get mangled.
            // Using raw_arg bypasses Rust's automatic quoting and sends the
            // string to CreateProcessW as-is.
            let mut c = tokio::process::Command::new("cmd");
            c.raw_arg(format!("/C {}", command));
            c
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut c = tokio::process::Command::new("sh");
            c.arg("-c").arg(command);
            c
        }
    };

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    if let Some(vars) = env_vars {
        for (k, v) in vars {
            cmd.env(k, v);
        }
    }

    if let Some((k, v)) = extra_env {
        cmd.env(k, v);
    }

    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    // Spawn the hook process
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return HookOutcome::Failure(format!("{} spawn error: {}", label, e), Vec::new());
        }
    };

    // Apply a 30-second timeout
    match tokio::time::timeout(std::time::Duration::from_secs(30), child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                HookOutcome::Success(output.stdout)
            } else {
                let code = output.status.code().unwrap_or(-1);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let detail = if stderr.trim().is_empty() {
                    format!("exited with code {}", code)
                } else {
                    format!("exited with code {}: {}", code, stderr.trim())
                };
                HookOutcome::Failure(format!("{} {}", label, detail), output.stdout)
            }
        }
        Ok(Err(e)) => HookOutcome::Failure(format!("{} wait error: {}", label, e), Vec::new()),
        Err(_) => HookOutcome::Failure(format!("{} timed out after 30 seconds", label), Vec::new()),
    }
}

/// Extracted cost/usage summary from a Claude CLI NDJSON log.
///
/// All fields represent aggregates summed across all Claude CLI invocations in the log,
/// not a single invocation.
pub(crate) struct CostSummary {
    pub(crate) total_cost_usd: Option<f64>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) num_turns: Option<u32>,
    pub(crate) model: Option<String>,
    pub(crate) usage: Option<serde_json::Value>,
}

/// Extract cost data from a Claude CLI NDJSON log.
///
/// Claude CLI emits newline-delimited JSON events. The relevant events are:
/// - `{"type":"system","subtype":"init",...,"model":"<model>"}` — emitted at session start
/// - `{"type":"result",...,"total_cost_usd":...}` — emitted at session end
///
/// The entire log content is scanned in a single pass to support jobs with multiple Claude invocations.
///
/// Aggregation behavior:
/// - `total_cost_usd`, `duration_ms`, and `num_turns` are summed across all `"type":"result"` events.
/// - `usage` token fields are merged (summed) across all invocations.
/// - `model` is set to the first model found from `"type":"system"` events.
///
/// Known limitation: If different models are used across invocations in the same log, only the
/// first model encountered is reported.
///
/// Non-NDJSON lines are silently skipped. As a performance optimisation, `serde_json::from_str`
/// is only called on lines that contain the substring `"type"`.
pub(crate) fn extract_cost_from_log(log_content: &[u8]) -> CostSummary {
    let mut summary = CostSummary {
        total_cost_usd: None,
        duration_ms: None,
        num_turns: None,
        model: None,
        usage: None,
    };

    if log_content.is_empty() {
        return summary;
    }

    let content_str = String::from_utf8_lossy(log_content);

    for line in content_str.lines() {
        let line = line.trim();
        if line.is_empty() || !line.contains("\"type\"") {
            continue;
        }

        let val = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        match val.get("type").and_then(|t| t.as_str()) {
            Some("system") if summary.model.is_none() => {
                // Keep the first model seen; continue scanning for result events.
                if let Some(model) = val.get("model").and_then(|m| m.as_str()) {
                    summary.model = Some(model.to_string());
                }
            }
            Some("result") => {
                if let Some(cost) = val.get("total_cost_usd").and_then(|v| v.as_f64()) {
                    *summary.total_cost_usd.get_or_insert(0.0) += cost;
                }
                if let Some(ms) = val.get("duration_ms").and_then(|v| v.as_u64()) {
                    *summary.duration_ms.get_or_insert(0) += ms;
                }
                if let Some(turns) = val.get("num_turns").and_then(|v| v.as_u64()) {
                    *summary.num_turns.get_or_insert(0) += turns as u32;
                }
                if let Some(serde_json::Value::Object(new_usage)) = val.get("usage").cloned() {
                    let merged = summary
                        .usage
                        .get_or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let serde_json::Value::Object(ref mut acc) = merged {
                        for (k, v) in new_usage {
                            if let Some(n) = v.as_f64() {
                                let entry = acc.entry(k).or_insert(serde_json::Value::Number(
                                    serde_json::Number::from(0u64),
                                ));
                                let existing = entry.as_f64().unwrap_or(0.0);
                                // Preserve integer representation where possible.
                                let sum = existing + n;
                                *entry = if sum.fract() == 0.0 && sum >= 0.0 {
                                    serde_json::Value::Number(serde_json::Number::from(sum as u64))
                                } else {
                                    serde_json::Value::Number(
                                        serde_json::Number::from_f64(sum)
                                            .unwrap_or(serde_json::Number::from(0u64)),
                                    )
                                };
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    summary
}

/// The reason a run was killed, carried over the kill channel so the executor
/// can set the correct error message on the resulting `JobRun` record.
#[derive(Debug, Clone, Copy)]
pub enum KillReason {
    /// A new run was dispatched for a job that does not allow concurrent execution.
    Concurrent,
    /// The daemon is shutting down.
    Shutdown,
    /// The run was killed manually via the API.
    Manual,
}

/// Handle to a running job, allowing monitoring and cancellation.
pub struct RunHandle {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub join_handle: tokio::task::JoinHandle<()>,
    pub kill_tx: oneshot::Sender<KillReason>,
}

/// The Executor spawns jobs using a PTY and manages the lifecycle.
pub struct Executor {
    event_tx: broadcast::Sender<JobEvent>,
    log_store: Arc<dyn LogStore>,
    config: Arc<DaemonConfig>,
    pty_spawner: Arc<dyn PtySpawner>,
}

impl Executor {
    /// Create a new Executor.
    pub fn new(
        event_tx: broadcast::Sender<JobEvent>,
        log_store: Arc<dyn LogStore>,
        config: Arc<DaemonConfig>,
        pty_spawner: Arc<dyn PtySpawner>,
    ) -> Self {
        Self {
            event_tx,
            log_store,
            config,
            pty_spawner,
        }
    }

    /// Build a CommandBuilder from the job's execution type.
    /// If trigger_args is provided, it is appended to the command string.
    /// If trigger_env is provided, those vars are applied after job env_vars (highest precedence).
    fn build_command(
        job: &Job,
        trigger_args: Option<&str>,
        trigger_env: Option<&HashMap<String, String>>,
    ) -> portable_pty::CommandBuilder {
        let mut cmd = match &job.execution {
            ExecutionType::ShellCommand(command) => {
                let effective_command = match trigger_args {
                    Some(args) => {
                        let sanitized = args.replace(['\n', '\r'], " ");
                        format!("{} {}", command, sanitized)
                    }
                    None => command.clone(),
                };
                if cfg!(target_os = "windows") {
                    let mut cb = portable_pty::CommandBuilder::new("cmd.exe");
                    cb.arg("/C");
                    cb.arg(&effective_command);
                    cb
                } else {
                    let mut cb = portable_pty::CommandBuilder::new("/bin/sh");
                    cb.arg("-c");
                    cb.arg(&effective_command);
                    cb
                }
            }
            ExecutionType::ScriptFile(script) => {
                let effective_script = match trigger_args {
                    Some(args) => {
                        let sanitized = args.replace(['\n', '\r'], " ");
                        format!("{} {}", script, sanitized)
                    }
                    None => script.clone(),
                };
                if cfg!(target_os = "windows") {
                    // Detect file extension
                    let ext = std::path::Path::new(script)
                        .extension()
                        .and_then(|e| e.to_str())
                        .unwrap_or("")
                        .to_lowercase();

                    match ext.as_str() {
                        "ps1" => {
                            let mut cb = portable_pty::CommandBuilder::new("powershell.exe");
                            if trigger_args.is_some() {
                                cb.arg("-Command");
                            } else {
                                cb.arg("-File");
                            }
                            cb.arg(&effective_script);
                            cb
                        }
                        _ => {
                            let mut cb = portable_pty::CommandBuilder::new("cmd.exe");
                            cb.arg("/C");
                            cb.arg(&effective_script);
                            cb
                        }
                    }
                } else {
                    let mut cb = portable_pty::CommandBuilder::new("/bin/sh");
                    if trigger_args.is_some() {
                        // With trigger args, use -c so the shell parses the concatenated string
                        cb.arg("-c");
                    }
                    cb.arg(&effective_script);
                    cb
                }
            }
        };

        // Set working directory if specified
        if let Some(ref dir) = job.working_dir {
            cmd.cwd(dir);
        }

        // Set environment variables if specified (job-level)
        if let Some(ref env_vars) = job.env_vars {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        // Merge trigger-time env vars (highest precedence for this run)
        if let Some(t_env) = trigger_env {
            for (key, value) in t_env {
                cmd.env(key, value);
            }
        }

        cmd
    }

    /// Spawn a job, returning a RunHandle for monitoring and cancellation.
    pub async fn spawn_job(
        &self,
        job: &Job,
        run_id: Uuid,
        trigger_params: Option<&TriggerParams>,
    ) -> anyhow::Result<RunHandle> {
        let job_id = job.id;
        let job_name = job.name.clone();
        let now = Utc::now();

        // Create a JobRun with Running status
        let run = JobRun {
            run_id,
            job_id,
            started_at: now,
            finished_at: None,
            status: RunStatus::Running,
            exit_code: None,
            log_size_bytes: 0,
            error: None,
            trigger_params: trigger_params.cloned(),
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        };

        // Save the initial run to the log store
        self.log_store.create_run(&run).await?;

        // Broadcast Started event
        let _ = self.event_tx.send(JobEvent::Started {
            job_id,
            run_id,
            job_name: job_name.clone(),
            timestamp: now,
        });

        // Resolve effective hooks: job-level overrides config default; empty string = no hook
        let effective_pre_hook = job
            .pre_hook
            .clone()
            .or_else(|| self.config.default_pre_hook.clone())
            .filter(|h| !h.is_empty());
        let effective_post_hook = job
            .post_hook
            .clone()
            .or_else(|| self.config.default_post_hook.clone())
            .filter(|h| !h.is_empty());

        // Build the command
        let cmd = Self::build_command(
            job,
            trigger_params.and_then(|p| p.args.as_deref()),
            trigger_params.and_then(|p| p.env.as_ref()),
        );

        // Clone things for the spawned task
        let execution = job.execution.clone();
        let log_environment = job.log_environment;
        let job_env_vars = job.env_vars.clone();
        let job_working_dir = job.working_dir.clone();
        let trigger_input = trigger_params.and_then(|p| p.input.clone());
        let trigger_args = trigger_params.and_then(|p| p.args.clone());
        let trigger_params_owned = trigger_params.cloned();
        let event_tx = self.event_tx.clone();
        let log_store = Arc::clone(&self.log_store);
        let pty_spawner = Arc::clone(&self.pty_spawner);
        let pty_rows = self.config.pty_rows;
        let pty_cols = self.config.pty_cols;

        // Compute effective timeout
        let effective_timeout_secs = if job.timeout_secs > 0 {
            job.timeout_secs
        } else {
            self.config.default_timeout_secs
        };
        let max_log_files = self.config.max_log_files_per_job;

        // Create kill channel
        let (kill_tx, kill_rx) = oneshot::channel::<KillReason>();

        // Spawn the execution task
        let join_handle = tokio::spawn(async move {
            // Track bytes written by hooks (outside the PTY log writer) so they
            // can be added to total_bytes after the log writer finishes.
            let mut pre_hook_stdout_len: u64 = 0;

            // Run pre-hook if configured (before PTY spawn)
            if let Some(ref hook_cmd) = effective_pre_hook {
                tracing::debug!("Running pre-hook for job {}: {}", job_id, hook_cmd);
                match run_hook(
                    hook_cmd,
                    job_working_dir.as_deref(),
                    job_env_vars.as_ref(),
                    None,
                    "pre-hook",
                )
                .await
                {
                    HookOutcome::Success(stdout) => {
                        tracing::debug!("Pre-hook succeeded for job {}", job_id);
                        let start_marker =
                            "================= PRE RUN HOOK START =================\n";
                        let end_marker = "================= PRE RUN HOOK END ===================\n";
                        log_store
                            .append_log(job_id, run_id, start_marker.as_bytes())
                            .await
                            .ok();
                        pre_hook_stdout_len += start_marker.len() as u64;
                        if !stdout.is_empty() {
                            pre_hook_stdout_len += stdout.len() as u64;
                            log_store.append_log(job_id, run_id, &stdout).await.ok();
                        }
                        log_store
                            .append_log(job_id, run_id, end_marker.as_bytes())
                            .await
                            .ok();
                        pre_hook_stdout_len += end_marker.len() as u64;
                    }
                    HookOutcome::Failure(detail, stdout) => {
                        let error_msg = format!("Pre-hook failed: {}", detail);
                        tracing::warn!("{} for job {}", error_msg, job_id);

                        let start_marker =
                            "================= PRE RUN HOOK START =================\n";
                        let end_marker = "================= PRE RUN HOOK END ===================\n";
                        log_store
                            .append_log(job_id, run_id, start_marker.as_bytes())
                            .await
                            .ok();

                        // Append any stdout from the failing pre-hook before building failed_run
                        let pre_hook_fail_stdout_len: u64 = if !stdout.is_empty() {
                            let len = stdout.len() as u64;
                            log_store.append_log(job_id, run_id, &stdout).await.ok();
                            len
                        } else {
                            0
                        };

                        log_store
                            .append_log(job_id, run_id, end_marker.as_bytes())
                            .await
                            .ok();
                        let pre_hook_fail_stdout_len = pre_hook_fail_stdout_len
                            + start_marker.len() as u64
                            + end_marker.len() as u64;

                        // Extract cost data from whatever was appended (best-effort)
                        let (
                            fail_cost_usd,
                            fail_duration_ms,
                            fail_num_turns,
                            fail_model,
                            fail_usage,
                        ) = if pre_hook_fail_stdout_len > 0 {
                            let raw = log_store
                                .read_log(job_id, run_id, None)
                                .await
                                .unwrap_or_default();
                            let c = extract_cost_from_log(raw.as_bytes());
                            (
                                c.total_cost_usd,
                                c.duration_ms,
                                c.num_turns,
                                c.model,
                                c.usage,
                            )
                        } else {
                            (None, None, None, None, None)
                        };

                        let failed_run = JobRun {
                            run_id,
                            job_id,
                            started_at: now,
                            finished_at: Some(Utc::now()),
                            status: RunStatus::Failed,
                            exit_code: None,
                            log_size_bytes: pre_hook_fail_stdout_len,
                            error: Some(error_msg.clone()),
                            trigger_params: trigger_params_owned.clone(),
                            total_cost_usd: fail_cost_usd,
                            duration_ms: fail_duration_ms,
                            num_turns: fail_num_turns,
                            model: fail_model,
                            usage: fail_usage,
                        };
                        if let Err(e) = log_store.update_run(&failed_run).await {
                            tracing::error!("Failed to save run on pre-hook failure: {}", e);
                        }
                        if let Err(e) = log_store.update_manifest(job_id, &failed_run).await {
                            tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                        }
                        let _ = event_tx.send(JobEvent::Completed {
                            job_id,
                            run_id,
                            exit_code: -1,
                            timestamp: Utc::now(),
                        });
                        // Cleanup old log files
                        if let Err(e) = log_store.cleanup(job_id, max_log_files).await {
                            tracing::error!("Failed to cleanup logs for job {}: {}", job_id, e);
                        }
                        return;
                    }
                }
            }

            // Try to spawn the process
            let spawn_result = {
                let spawner = pty_spawner;
                let cmd = cmd;
                spawner.spawn(cmd, pty_rows, pty_cols)
            };

            let mut process = match spawn_result {
                Ok(process) => process,
                Err(e) => {
                    let error_msg = format!("Failed to spawn process: {}", e);
                    tracing::error!("{}", error_msg);

                    // Broadcast Failed event
                    let _ = event_tx.send(JobEvent::Failed {
                        job_id,
                        run_id,
                        error: error_msg.clone(),
                        timestamp: Utc::now(),
                    });

                    // Update the run to Failed status
                    let failed_run = JobRun {
                        run_id,
                        job_id,
                        started_at: now,
                        finished_at: Some(Utc::now()),
                        status: RunStatus::Failed,
                        exit_code: None,
                        log_size_bytes: 0,
                        error: Some(error_msg),
                        trigger_params: trigger_params_owned.clone(),
                        total_cost_usd: None,
                        duration_ms: None,
                        num_turns: None,
                        model: None,
                        usage: None,
                    };
                    if let Err(e) = log_store.update_run(&failed_run).await {
                        tracing::error!("Failed to update run on spawn failure: {}", e);
                    }
                    if let Err(e) = log_store.update_manifest(job_id, &failed_run).await {
                        tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                    }

                    // Cleanup old log files
                    if let Err(e) = log_store.cleanup(job_id, max_log_files).await {
                        tracing::error!("Failed to cleanup logs for job {}: {}", job_id, e);
                    }
                    return;
                }
            };

            // Write trigger input to stdin if provided, then always close stdin
            // to signal EOF. Without this, piped stdin would hang processes that
            // read from stdin (e.g. claude CLI detecting a pipe).
            if let Some(ref input_data) = trigger_input {
                if let Err(e) = process.write_stdin(input_data.as_bytes()) {
                    tracing::warn!("Failed to write trigger input to stdin: {}", e);
                }
            }
            process.close_stdin();

            // If log_environment is enabled, dump full environment before command
            if log_environment {
                let mut env_map: std::collections::BTreeMap<String, String> =
                    std::env::vars().collect();
                // Merge job-specific env vars (these override inherited ones)
                if let Some(ref job_envs) = job_env_vars {
                    for (k, v) in job_envs {
                        env_map.insert(k.clone(), v.clone());
                    }
                }
                // Merge trigger-level env vars (highest precedence)
                if let Some(ref tp) = trigger_params_owned {
                    if let Some(ref t_env) = tp.env {
                        for (k, v) in t_env {
                            env_map.insert(k.clone(), v.clone());
                        }
                    }
                }
                let mut env_dump = String::from("=== Environment ===\n");
                for (key, value) in &env_map {
                    env_dump.push_str(&format!("{}={}\n", key, value));
                }
                env_dump.push_str("===================\n");
                let _ = log_store
                    .append_log(job_id, run_id, env_dump.as_bytes())
                    .await;
                let _ = event_tx.send(JobEvent::Output {
                    job_id,
                    run_id,
                    data: Arc::from(env_dump.as_str()),
                    timestamp: Utc::now(),
                });
            }

            // Write JOB RUN START marker and command header to log
            let job_run_start_marker = "================= JOB RUN START ======================\n";
            let _ = log_store
                .append_log(job_id, run_id, job_run_start_marker.as_bytes())
                .await;
            let _ = event_tx.send(JobEvent::Output {
                job_id,
                run_id,
                data: Arc::from(job_run_start_marker),
                timestamp: Utc::now(),
            });

            // Write command header to log (effective command with trigger args)
            let command_str = match &execution {
                ExecutionType::ShellCommand(cmd) => match &trigger_args {
                    Some(args) => format!("{} {}", cmd, args),
                    None => cmd.clone(),
                },
                ExecutionType::ScriptFile(script) => match &trigger_args {
                    Some(args) => format!("[script] {} {}", script, args),
                    None => format!("[script] {}", script),
                },
            };
            let header = format!("$ {}\n", command_str);
            let _ = log_store
                .append_log(job_id, run_id, header.as_bytes())
                .await;
            let _ = event_tx.send(JobEvent::Output {
                job_id,
                run_id,
                data: Arc::from(header.as_str()),
                timestamp: Utc::now(),
            });

            // Create mpsc channel for log writer (capacity 256 per SPEC)
            let (log_tx, log_rx) = mpsc::channel::<Vec<u8>>(256);

            // Spawn log writer task
            let log_store_writer = Arc::clone(&log_store);
            let log_writer_handle = tokio::spawn(async move {
                let mut rx = log_rx;
                let mut total_bytes: u64 = 0;
                while let Some(data) = rx.recv().await {
                    total_bytes += data.len() as u64;
                    if let Err(e) = log_store_writer.append_log(job_id, run_id, &data).await {
                        tracing::error!("Failed to append log: {}", e);
                    }
                }
                total_bytes
            });

            // Create a channel to receive output from spawn_blocking
            let (output_tx, mut output_rx) = mpsc::channel::<Vec<u8>>(256);

            // Spawn blocking PTY read loop
            let read_handle = tokio::task::spawn_blocking(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match process.read(&mut buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            let data = buf[..n].to_vec();
                            if output_tx.blocking_send(data).is_err() {
                                break; // Receiver dropped
                            }
                        }
                        Err(e) => {
                            // On Windows, ConPTY may return error when process exits
                            tracing::debug!("PTY read error (may be expected at EOF): {}", e);
                            break;
                        }
                    }
                }
                // Wait for exit status
                process.wait()
            });

            // Forward output: broadcast events and send to log writer
            let event_tx_output = event_tx.clone();
            let log_tx_output = log_tx;

            // Process output chunks - use select to handle kill signal and timeout
            let mut kill_rx = kill_rx;
            let mut killed = false;
            let mut kill_reason: Option<KillReason> = None;
            let mut timed_out = false;

            // Create timeout future if timeout is configured
            let timeout_fut = if effective_timeout_secs > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(effective_timeout_secs))
            } else {
                // Effectively infinite sleep (136 years)
                tokio::time::sleep(std::time::Duration::from_secs(u64::MAX / 2))
            };
            tokio::pin!(timeout_fut);

            loop {
                tokio::select! {
                    chunk = output_rx.recv() => {
                        match chunk {
                            Some(data) => {
                                // Convert to lossy UTF-8 for broadcast
                                let text = String::from_utf8_lossy(&data);
                                let arc_str: Arc<str> = Arc::from(text.as_ref());

                                // Broadcast Output event
                                let _ = event_tx_output.send(JobEvent::Output {
                                    job_id,
                                    run_id,
                                    data: arc_str,
                                    timestamp: Utc::now(),
                                });

                                // Send raw bytes to log writer
                                let _ = log_tx_output.send(data).await;
                            }
                            None => break, // PTY read loop ended
                        }
                    }
                    reason = &mut kill_rx => {
                        killed = true;
                        kill_reason = reason.ok();
                        break;
                    }
                    _ = &mut timeout_fut => {
                        timed_out = true;
                        break;
                    }
                }
            }

            // Drop log_tx to signal log writer to finish
            drop(log_tx_output);

            // Wait for the read handle to complete and get exit status
            let exit_result = read_handle.await;

            // Wait for log writer to finish and get total bytes
            let mut total_bytes: u64 = (log_writer_handle.await).unwrap_or_default();
            // Include bytes written directly by the pre-hook (outside the log writer)
            total_bytes += pre_hook_stdout_len;
            // Include bytes written directly for the JOB RUN START marker and command header
            total_bytes += job_run_start_marker.len() as u64;
            total_bytes += header.len() as u64;

            // Write JOB RUN END marker after PTY output completes
            let job_run_end_marker = "================= JOB RUN END ========================\n";
            log_store
                .append_log(job_id, run_id, job_run_end_marker.as_bytes())
                .await
                .ok();
            total_bytes += job_run_end_marker.len() as u64;

            let finished_at = Utc::now();

            // Extract cost data from the NDJSON log (best-effort; errors are silently ignored).
            // Fields are stored as individual variables so each terminal branch can use them
            // even though some branches return early.
            let (cost_total_usd, cost_duration_ms, cost_num_turns, cost_model, cost_usage) = {
                let raw = log_store
                    .read_log(job_id, run_id, None)
                    .await
                    .unwrap_or_default();
                let c = extract_cost_from_log(raw.as_bytes());
                (
                    c.total_cost_usd,
                    c.duration_ms,
                    c.num_turns,
                    c.model,
                    c.usage,
                )
            };

            if timed_out {
                // Job timed out - mark as Failed with timeout message
                let timeout_run = JobRun {
                    run_id,
                    job_id,
                    started_at: now,
                    finished_at: Some(finished_at),
                    status: RunStatus::Failed,
                    exit_code: None,
                    log_size_bytes: total_bytes,
                    error: Some("execution timed out".to_string()),
                    trigger_params: trigger_params_owned.clone(),
                    total_cost_usd: cost_total_usd,
                    duration_ms: cost_duration_ms,
                    num_turns: cost_num_turns,
                    model: cost_model.clone(),
                    usage: cost_usage.clone(),
                };
                if let Err(e) = log_store.update_run(&timeout_run).await {
                    tracing::error!("Failed to update run on timeout: {}", e);
                }
                if let Err(e) = log_store.update_manifest(job_id, &timeout_run).await {
                    tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                }
                let _ = event_tx.send(JobEvent::Failed {
                    job_id,
                    run_id,
                    error: "execution timed out".to_string(),
                    timestamp: finished_at,
                });

                // Cleanup old log files
                if let Err(e) = log_store.cleanup(job_id, max_log_files).await {
                    tracing::error!("Failed to cleanup logs for job {}: {}", job_id, e);
                }
                return;
            }

            if killed {
                // Job was killed — derive the error message from the kill reason.
                let kill_error_msg = match kill_reason {
                    Some(KillReason::Concurrent) => {
                        "Run killed: concurrent execution is disabled for this job".to_string()
                    }
                    Some(KillReason::Shutdown) => "Daemon shutting down".to_string(),
                    Some(KillReason::Manual) | None => "Job was killed".to_string(),
                };
                let killed_run = JobRun {
                    run_id,
                    job_id,
                    started_at: now,
                    finished_at: Some(finished_at),
                    status: RunStatus::Killed,
                    exit_code: Some(-1),
                    log_size_bytes: total_bytes,
                    error: Some(kill_error_msg.clone()),
                    trigger_params: trigger_params_owned.clone(),
                    total_cost_usd: cost_total_usd,
                    duration_ms: cost_duration_ms,
                    num_turns: cost_num_turns,
                    model: cost_model.clone(),
                    usage: cost_usage.clone(),
                };
                if let Err(e) = log_store.update_run(&killed_run).await {
                    tracing::error!("Failed to update run on kill: {}", e);
                }
                if let Err(e) = log_store.update_manifest(job_id, &killed_run).await {
                    tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                }
                let _ = event_tx.send(JobEvent::Failed {
                    job_id,
                    run_id,
                    error: kill_error_msg,
                    timestamp: finished_at,
                });

                // Cleanup old log files
                if let Err(e) = log_store.cleanup(job_id, max_log_files).await {
                    tracing::error!("Failed to cleanup logs for job {}: {}", job_id, e);
                }
                return;
            }

            // Process the exit result
            match exit_result {
                Ok(Ok(status)) => {
                    // Get exit code
                    let exit_code = status.code().unwrap_or(-1);

                    // Run post-hook if configured
                    let mut post_hook_stdout_len: u64 = 0;
                    let (final_status, hook_error) = if let Some(ref hook_cmd) = effective_post_hook
                    {
                        tracing::debug!("Running post-hook for job {}: {}", job_id, hook_cmd);
                        let exit_code_str = exit_code.to_string();
                        match run_hook(
                            hook_cmd,
                            job_working_dir.as_deref(),
                            job_env_vars.as_ref(),
                            Some(("ACS_JOB_EXIT_CODE", &exit_code_str)),
                            "post-hook",
                        )
                        .await
                        {
                            HookOutcome::Success(stdout) => {
                                tracing::debug!("Post-hook succeeded for job {}", job_id);
                                let start_marker =
                                    "================= POST RUN HOOK START ================\n";
                                let end_marker =
                                    "================= POST RUN HOOK END ==================\n";
                                log_store
                                    .append_log(job_id, run_id, start_marker.as_bytes())
                                    .await
                                    .ok();
                                post_hook_stdout_len += start_marker.len() as u64;
                                if !stdout.is_empty() {
                                    post_hook_stdout_len += stdout.len() as u64;
                                    log_store.append_log(job_id, run_id, &stdout).await.ok();
                                }
                                log_store
                                    .append_log(job_id, run_id, end_marker.as_bytes())
                                    .await
                                    .ok();
                                post_hook_stdout_len += end_marker.len() as u64;
                                (RunStatus::Completed, None)
                            }
                            HookOutcome::Failure(detail, stdout) => {
                                let error_msg = format!("Post-hook failed: {}", detail);
                                tracing::warn!("{} for job {}", error_msg, job_id);
                                let start_marker =
                                    "================= POST RUN HOOK START ================\n";
                                let end_marker =
                                    "================= POST RUN HOOK END ==================\n";
                                log_store
                                    .append_log(job_id, run_id, start_marker.as_bytes())
                                    .await
                                    .ok();
                                post_hook_stdout_len += start_marker.len() as u64;
                                if !stdout.is_empty() {
                                    post_hook_stdout_len += stdout.len() as u64;
                                    log_store.append_log(job_id, run_id, &stdout).await.ok();
                                }
                                log_store
                                    .append_log(job_id, run_id, end_marker.as_bytes())
                                    .await
                                    .ok();
                                post_hook_stdout_len += end_marker.len() as u64;
                                (RunStatus::CompletedWithWarnings, Some(error_msg))
                            }
                        }
                    } else {
                        (RunStatus::Completed, None)
                    };

                    // Add post-hook stdout bytes to total log size
                    total_bytes += post_hook_stdout_len;

                    // Re-extract cost to include any hook costs appended to the log
                    let (cost_total_usd, cost_duration_ms, cost_num_turns, cost_model, cost_usage) =
                        if post_hook_stdout_len > 0 {
                            let raw = log_store
                                .read_log(job_id, run_id, None)
                                .await
                                .unwrap_or_default();
                            let c = extract_cost_from_log(raw.as_bytes());
                            (
                                c.total_cost_usd,
                                c.duration_ms,
                                c.num_turns,
                                c.model,
                                c.usage,
                            )
                        } else {
                            (
                                cost_total_usd,
                                cost_duration_ms,
                                cost_num_turns,
                                cost_model,
                                cost_usage,
                            )
                        };

                    // Per SPEC: non-zero exit is Completed (not Failed).
                    // Failed = infrastructure error only.
                    let completed_run = JobRun {
                        run_id,
                        job_id,
                        started_at: now,
                        finished_at: Some(finished_at),
                        status: final_status,
                        exit_code: Some(exit_code),
                        log_size_bytes: total_bytes,
                        error: hook_error,
                        trigger_params: trigger_params_owned.clone(),
                        total_cost_usd: cost_total_usd,
                        duration_ms: cost_duration_ms,
                        num_turns: cost_num_turns,
                        model: cost_model.clone(),
                        usage: cost_usage.clone(),
                    };
                    if let Err(e) = log_store.update_run(&completed_run).await {
                        tracing::error!("Failed to update run on completion: {}", e);
                    }
                    if let Err(e) = log_store.update_manifest(job_id, &completed_run).await {
                        tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                    }

                    let _ = event_tx.send(JobEvent::Completed {
                        job_id,
                        run_id,
                        exit_code,
                        timestamp: finished_at,
                    });
                }
                Ok(Err(e)) => {
                    // Process wait failed - infrastructure error
                    let error_msg = format!("Process wait failed: {}", e);
                    let failed_run = JobRun {
                        run_id,
                        job_id,
                        started_at: now,
                        finished_at: Some(finished_at),
                        status: RunStatus::Failed,
                        exit_code: None,
                        log_size_bytes: total_bytes,
                        error: Some(error_msg.clone()),
                        trigger_params: trigger_params_owned.clone(),
                        total_cost_usd: cost_total_usd,
                        duration_ms: cost_duration_ms,
                        num_turns: cost_num_turns,
                        model: cost_model.clone(),
                        usage: cost_usage.clone(),
                    };
                    if let Err(e) = log_store.update_run(&failed_run).await {
                        tracing::error!("Failed to update run on wait failure: {}", e);
                    }
                    if let Err(e) = log_store.update_manifest(job_id, &failed_run).await {
                        tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                    }

                    let _ = event_tx.send(JobEvent::Failed {
                        job_id,
                        run_id,
                        error: error_msg,
                        timestamp: finished_at,
                    });
                }
                Err(e) => {
                    // JoinError from spawn_blocking
                    let error_msg = format!("Task join error: {}", e);
                    let failed_run = JobRun {
                        run_id,
                        job_id,
                        started_at: now,
                        finished_at: Some(finished_at),
                        status: RunStatus::Failed,
                        exit_code: None,
                        log_size_bytes: total_bytes,
                        error: Some(error_msg.clone()),
                        trigger_params: trigger_params_owned,
                        total_cost_usd: cost_total_usd,
                        duration_ms: cost_duration_ms,
                        num_turns: cost_num_turns,
                        model: cost_model,
                        usage: cost_usage,
                    };
                    if let Err(e) = log_store.update_run(&failed_run).await {
                        tracing::error!("Failed to update run on join error: {}", e);
                    }
                    if let Err(e) = log_store.update_manifest(job_id, &failed_run).await {
                        tracing::warn!("Failed to update manifest for job {}: {}", job_id, e);
                    }

                    let _ = event_tx.send(JobEvent::Failed {
                        job_id,
                        run_id,
                        error: error_msg,
                        timestamp: finished_at,
                    });
                }
            }

            // Cleanup old log files after run completes
            if let Err(e) = log_store.cleanup(job_id, max_log_files).await {
                tracing::error!("Failed to cleanup logs for job {}: {}", job_id, e);
            }
        });

        Ok(RunHandle {
            run_id,
            job_id,
            join_handle,
            kill_tx,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ExecutionType;
    use crate::pty::MockPtySpawner;
    use crate::storage::LogStore;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use tokio::sync::RwLock;

    // --- InMemoryLogStore for testing ---

    struct InMemoryLogStore {
        runs: RwLock<Vec<JobRun>>,
        logs: RwLock<HashMap<(Uuid, Uuid), Vec<u8>>>,
        cleanup_calls: RwLock<Vec<(Uuid, usize)>>,
    }

    impl InMemoryLogStore {
        fn new() -> Self {
            Self {
                runs: RwLock::new(Vec::new()),
                logs: RwLock::new(HashMap::new()),
                cleanup_calls: RwLock::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl LogStore for InMemoryLogStore {
        async fn create_run(&self, run: &JobRun) -> anyhow::Result<()> {
            let mut runs = self.runs.write().await;
            runs.push(run.clone());
            Ok(())
        }

        async fn update_run(&self, run: &JobRun) -> anyhow::Result<()> {
            let mut runs = self.runs.write().await;
            if let Some(existing) = runs.iter_mut().find(|r| r.run_id == run.run_id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> anyhow::Result<()> {
            let mut logs = self.logs.write().await;
            let entry = logs.entry((job_id, run_id)).or_insert_with(Vec::new);
            entry.extend_from_slice(data);
            Ok(())
        }

        async fn read_log(
            &self,
            job_id: Uuid,
            run_id: Uuid,
            _tail: Option<usize>,
        ) -> anyhow::Result<String> {
            let logs = self.logs.read().await;
            match logs.get(&(job_id, run_id)) {
                Some(data) => Ok(String::from_utf8_lossy(data).to_string()),
                None => Ok(String::new()),
            }
        }

        async fn list_runs(
            &self,
            job_id: Uuid,
            limit: usize,
            offset: usize,
        ) -> anyhow::Result<(Vec<JobRun>, usize)> {
            let runs = self.runs.read().await;
            let filtered: Vec<JobRun> = runs
                .iter()
                .filter(|r| r.job_id == job_id)
                .cloned()
                .collect();
            let total = filtered.len();
            let paginated = filtered.into_iter().skip(offset).take(limit).collect();
            Ok((paginated, total))
        }

        async fn cleanup(&self, job_id: Uuid, max_files: usize) -> anyhow::Result<()> {
            self.cleanup_calls.write().await.push((job_id, max_files));
            Ok(())
        }

        async fn read_manifest(
            &self,
            _job_id: Uuid,
        ) -> anyhow::Result<Option<crate::models::JobManifest>> {
            Ok(None)
        }

        async fn update_manifest(&self, _job_id: Uuid, _run: &JobRun) -> anyhow::Result<()> {
            Ok(())
        }

        async fn rebuild_manifest(
            &self,
            _job_id: Uuid,
        ) -> anyhow::Result<crate::models::JobManifest> {
            Ok(crate::models::JobManifest::new(_job_id))
        }
    }

    // --- Test helpers ---

    fn make_test_job() -> Job {
        let now = Utc::now();
        Job {
            id: Uuid::now_v7(),
            name: "test-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        }
    }

    fn setup_executor(
        spawner: MockPtySpawner,
    ) -> (
        Executor,
        broadcast::Receiver<JobEvent>,
        Arc<InMemoryLogStore>,
    ) {
        let config = Arc::new(DaemonConfig::default());
        let (event_tx, event_rx) = broadcast::channel::<JobEvent>(4096);
        let log_store = Arc::new(InMemoryLogStore::new());
        let pty_spawner = Arc::new(spawner);

        let executor = Executor::new(
            event_tx,
            Arc::clone(&log_store) as Arc<dyn LogStore>,
            config,
            pty_spawner as Arc<dyn PtySpawner>,
        );

        (executor, event_rx, log_store)
    }

    // --- Executor tests ---

    #[tokio::test]
    async fn test_executor_output_hello_exit_zero() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"hello\n".to_vec()], 0);
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");

        // Wait for the task to complete
        handle.join_handle.await.expect("join");

        // Collect events
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Should have Started, Output, and Completed events
        assert!(
            events.len() >= 3,
            "Expected at least 3 events, got {}",
            events.len()
        );

        // Find the Completed event and verify exit_code=0
        let completed = events
            .iter()
            .find(|e| matches!(e, JobEvent::Completed { .. }));
        assert!(completed.is_some(), "Expected a Completed event");
        match completed.unwrap() {
            JobEvent::Completed { exit_code, .. } => {
                assert_eq!(*exit_code, 0);
            }
            _ => panic!("Expected Completed"),
        }
    }

    #[tokio::test]
    async fn test_executor_exit_one() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"error\n".to_vec()], 1);
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Per SPEC: non-zero exit is Completed (not Failed)
        let completed = events
            .iter()
            .find(|e| matches!(e, JobEvent::Completed { .. }));
        assert!(
            completed.is_some(),
            "Expected a Completed event for non-zero exit"
        );
        match completed.unwrap() {
            JobEvent::Completed { exit_code, .. } => {
                assert_eq!(*exit_code, 1);
            }
            _ => panic!("Expected Completed"),
        }
    }

    #[tokio::test]
    async fn test_executor_spawn_error() {
        let spawner = MockPtySpawner::with_spawn_error("PTY not available");
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Should have Started and Failed events
        let failed = events.iter().find(|e| matches!(e, JobEvent::Failed { .. }));
        assert!(failed.is_some(), "Expected a Failed event on spawn error");
        match failed.unwrap() {
            JobEvent::Failed { error, .. } => {
                assert!(
                    error.contains("PTY not available"),
                    "Error should mention PTY not available, got: {}",
                    error
                );
            }
            _ => panic!("Expected Failed"),
        }
    }

    #[tokio::test]
    async fn test_event_ordering_started_before_output_before_completed() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"hello\n".to_vec()], 0);
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Verify ordering: Started should come first
        assert!(
            matches!(events.first(), Some(JobEvent::Started { .. })),
            "First event should be Started, got: {:?}",
            events.first()
        );

        // Find position of Output and Completed events
        let output_pos = events
            .iter()
            .position(|e| matches!(e, JobEvent::Output { .. }));
        let completed_pos = events
            .iter()
            .position(|e| matches!(e, JobEvent::Completed { .. }));

        assert!(output_pos.is_some(), "Should have Output event");
        assert!(completed_pos.is_some(), "Should have Completed event");

        // Output should come before Completed
        assert!(
            output_pos.unwrap() < completed_pos.unwrap(),
            "Output should come before Completed"
        );
    }

    #[tokio::test]
    async fn test_output_chunking_multiple_events() {
        let spawner = MockPtySpawner::with_output_and_exit(
            vec![
                b"chunk1\n".to_vec(),
                b"chunk2\n".to_vec(),
                b"chunk3\n".to_vec(),
            ],
            0,
        );
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Count Output events (includes 1 delimiter + 1 command header + 3 chunks = 5)
        let output_count = events
            .iter()
            .filter(|e| matches!(e, JobEvent::Output { .. }))
            .count();

        assert_eq!(
            output_count, 5,
            "Expected 5 Output events (1 delimiter + 1 header + 3 chunks), got {}",
            output_count
        );
    }

    #[tokio::test]
    async fn test_log_writer_receives_all_output() {
        let spawner =
            MockPtySpawner::with_output_and_exit(vec![b"line1\n".to_vec(), b"line2\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let run_id = handle.run_id;
        let job_id = handle.job_id;

        handle.join_handle.await.expect("join");

        // Give a short delay to ensure log writer finishes
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify log content
        let log_content = log_store
            .read_log(job_id, run_id, None)
            .await
            .expect("read_log");

        assert!(
            log_content.contains("line1"),
            "Log should contain 'line1', got: {}",
            log_content
        );
        assert!(
            log_content.contains("line2"),
            "Log should contain 'line2', got: {}",
            log_content
        );
    }

    #[tokio::test]
    async fn test_executor_updates_run_on_completion() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let run_id = handle.run_id;

        handle.join_handle.await.expect("join");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Check the run was updated to Completed
        let runs = log_store.runs.read().await;
        let run = runs.iter().find(|r| r.run_id == run_id);
        assert!(run.is_some(), "Run should exist in log store");
        let run = run.unwrap();
        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.exit_code, Some(0));
        assert!(run.finished_at.is_some());
    }

    #[tokio::test]
    async fn test_executor_updates_run_on_spawn_failure() {
        let spawner = MockPtySpawner::with_spawn_error("spawn failed");
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let run_id = handle.run_id;

        handle.join_handle.await.expect("join");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Check the run was updated to Failed
        let runs = log_store.runs.read().await;
        let run = runs.iter().find(|r| r.run_id == run_id);
        assert!(run.is_some(), "Run should exist in log store");
        let run = run.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.error.is_some());
        assert!(run.error.as_ref().unwrap().contains("spawn failed"));
    }

    #[tokio::test]
    async fn test_executor_started_event_has_correct_fields() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![], 0);
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.name = "my-special-job".to_string();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let run_id = handle.run_id;

        handle.join_handle.await.expect("join");

        // Get the first event (should be Started)
        let event = event_rx.try_recv().expect("should have Started event");
        match event {
            JobEvent::Started {
                job_id,
                run_id: event_run_id,
                job_name,
                ..
            } => {
                assert_eq!(job_id, job.id);
                assert_eq!(event_run_id, run_id);
                assert_eq!(job_name, "my-special-job");
            }
            _ => panic!("Expected Started event"),
        }
    }

    #[tokio::test]
    async fn test_executor_no_output_still_completes() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![], 0);
        let (executor, mut event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Should have Started, JOB RUN START delimiter Output, command header Output, and Completed events
        let output_count = events
            .iter()
            .filter(|e| matches!(e, JobEvent::Output { .. }))
            .count();
        assert_eq!(
            output_count, 2,
            "Should have JOB RUN START delimiter and command header Output events"
        );

        let completed = events
            .iter()
            .find(|e| matches!(e, JobEvent::Completed { .. }));
        assert!(completed.is_some(), "Should have Completed event");
    }

    #[tokio::test]
    async fn test_build_command_shell_command() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "cmd-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello world".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, None, None);
        let args = cmd.get_argv();

        if cfg!(target_os = "windows") {
            assert_eq!(args[0].to_string_lossy(), "cmd.exe");
            assert_eq!(args[1].to_string_lossy(), "/C");
            assert_eq!(args[2].to_string_lossy(), "echo hello world");
        } else {
            assert_eq!(args[0].to_string_lossy(), "/bin/sh");
            assert_eq!(args[1].to_string_lossy(), "-c");
            assert_eq!(args[2].to_string_lossy(), "echo hello world");
        }
    }

    #[tokio::test]
    async fn test_build_command_script_file() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "script-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.sh".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, None, None);
        let args = cmd.get_argv();

        if cfg!(target_os = "windows") {
            assert_eq!(args[0].to_string_lossy(), "cmd.exe");
        } else {
            assert_eq!(args[0].to_string_lossy(), "/bin/sh");
            assert_eq!(args[1].to_string_lossy(), "deploy.sh");
        }
    }

    // --- Phase 8: Timeout enforcement tests ---

    fn setup_executor_with_timeout(
        spawner: MockPtySpawner,
        timeout_secs: u64,
    ) -> (
        Executor,
        broadcast::Receiver<JobEvent>,
        Arc<InMemoryLogStore>,
    ) {
        let config = DaemonConfig {
            default_timeout_secs: timeout_secs,
            ..Default::default()
        };
        let config = Arc::new(config);
        let (event_tx, event_rx) = broadcast::channel::<JobEvent>(4096);
        let log_store = Arc::new(InMemoryLogStore::new());
        let pty_spawner = Arc::new(spawner);

        let executor = Executor::new(
            event_tx,
            Arc::clone(&log_store) as Arc<dyn LogStore>,
            config,
            pty_spawner as Arc<dyn PtySpawner>,
        );

        (executor, event_rx, log_store)
    }

    #[tokio::test]
    async fn test_executor_timeout_kills_long_running_job() {
        // Create a mock that takes 5 seconds per chunk (way longer than 1s timeout)
        let spawner = MockPtySpawner::with_slow_output(
            vec![b"chunk1\n".to_vec(), b"chunk2\n".to_vec()],
            0,
            5000,
        );
        let (executor, mut event_rx, log_store) = setup_executor_with_timeout(spawner, 1);

        let mut job = make_test_job();
        job.timeout_secs = 0; // Use config default (1s)

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let run_id = handle.run_id;

        handle.join_handle.await.expect("join");

        // Collect events
        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        // Should have a Failed event with "execution timed out"
        let failed = events.iter().find(|e| matches!(e, JobEvent::Failed { .. }));
        assert!(failed.is_some(), "Expected a Failed event for timeout");
        match failed.unwrap() {
            JobEvent::Failed { error, .. } => {
                assert!(
                    error.contains("timed out"),
                    "Error should mention timeout, got: {}",
                    error
                );
            }
            _ => panic!("Expected Failed"),
        }

        // Verify run status is Failed
        let runs = log_store.runs.read().await;
        let run = runs.iter().find(|r| r.run_id == run_id);
        assert!(run.is_some());
        let run = run.unwrap();
        assert_eq!(run.status, RunStatus::Failed);
        assert!(run.error.as_ref().unwrap().contains("timed out"));
        assert!(run.exit_code.is_none());
    }

    #[tokio::test]
    async fn test_executor_timeout_uses_job_timeout_over_config() {
        // Config has 100s timeout, but job has 1s
        let spawner = MockPtySpawner::with_slow_output(vec![b"slow\n".to_vec()], 0, 5000);
        let (executor, mut event_rx, _log_store) = setup_executor_with_timeout(spawner, 100);

        let mut job = make_test_job();
        job.timeout_secs = 1; // Override with 1s

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        let failed = events.iter().find(|e| matches!(e, JobEvent::Failed { .. }));
        assert!(
            failed.is_some(),
            "Job should have timed out using job-level timeout"
        );
    }

    #[tokio::test]
    async fn test_executor_no_timeout_when_zero() {
        // Both config and job have 0 timeout - should complete normally
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"hello\n".to_vec()], 0);
        let (executor, mut event_rx, _log_store) = setup_executor_with_timeout(spawner, 0);

        let job = make_test_job(); // timeout_secs = 0
        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        let mut events = Vec::new();
        while let Ok(event) = event_rx.try_recv() {
            events.push(event);
        }

        let completed = events
            .iter()
            .find(|e| matches!(e, JobEvent::Completed { .. }));
        assert!(
            completed.is_some(),
            "Job should complete normally with no timeout"
        );
    }

    // --- Phase 8: Log cleanup after run tests ---

    #[tokio::test]
    async fn test_executor_calls_cleanup_after_completion() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let job_id = handle.job_id;
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify cleanup was called
        let calls = log_store.cleanup_calls.read().await;
        assert!(
            !calls.is_empty(),
            "cleanup should have been called after run completes"
        );
        assert_eq!(
            calls[0].0, job_id,
            "cleanup should be called with the correct job_id"
        );
        assert_eq!(
            calls[0].1, 50,
            "cleanup should use default max_log_files_per_job"
        );
    }

    #[tokio::test]
    async fn test_executor_calls_cleanup_after_spawn_failure() {
        let spawner = MockPtySpawner::with_spawn_error("spawn failed");
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let handle = executor
            .spawn_job(&job, Uuid::now_v7(), None)
            .await
            .expect("spawn_job");
        let job_id = handle.job_id;
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Verify cleanup was called even on spawn failure
        let calls = log_store.cleanup_calls.read().await;
        assert!(
            !calls.is_empty(),
            "cleanup should have been called after spawn failure"
        );
        assert_eq!(calls[0].0, job_id);
    }

    #[tokio::test]
    async fn test_build_command_with_trigger_args_shell() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "args-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--extra flag"), None);
        let args = cmd.get_argv();

        assert_eq!(args[2].to_string_lossy(), "echo hello --extra flag");
    }

    #[tokio::test]
    async fn test_build_command_with_trigger_args_script() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "script-args-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.sh".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--env prod"), None);
        let args = cmd.get_argv();

        if cfg!(target_os = "windows") {
            // cmd.exe /C "deploy.sh --env prod"
            assert!(args[2].to_string_lossy().contains("deploy.sh --env prod"));
        } else {
            // /bin/sh -c "deploy.sh --env prod"
            assert_eq!(args[1].to_string_lossy(), "-c");
            assert_eq!(args[2].to_string_lossy(), "deploy.sh --env prod");
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn test_build_command_script_with_trigger_args_unix() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "unix-script-args".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.sh".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--flag"), None);
        let args = cmd.get_argv();

        // Should produce ["/bin/sh", "-c", "deploy.sh --flag"]
        assert_eq!(args.len(), 3, "Expected 3 args, got {:?}", args);
        assert_eq!(args[0].to_string_lossy(), "/bin/sh");
        assert_eq!(args[1].to_string_lossy(), "-c");
        assert_eq!(args[2].to_string_lossy(), "deploy.sh --flag");
    }

    #[cfg(not(target_os = "windows"))]
    #[tokio::test]
    async fn test_build_command_script_without_trigger_args_unix() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "unix-script-no-args".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.sh".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, None, None);
        let args = cmd.get_argv();

        // Should produce ["/bin/sh", "deploy.sh"] (unchanged behavior)
        assert_eq!(args.len(), 2, "Expected 2 args, got {:?}", args);
        assert_eq!(args[0].to_string_lossy(), "/bin/sh");
        assert_eq!(args[1].to_string_lossy(), "deploy.sh");
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_build_command_script_with_trigger_args_windows_ps1() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "win-ps1-args".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.ps1".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--flag"), None);
        let args = cmd.get_argv();

        // Should produce ["powershell.exe", "-Command", "deploy.ps1 --flag"]
        assert_eq!(args.len(), 3, "Expected 3 args, got {:?}", args);
        assert_eq!(args[0].to_string_lossy(), "powershell.exe");
        assert_eq!(args[1].to_string_lossy(), "-Command");
        assert_eq!(args[2].to_string_lossy(), "deploy.ps1 --flag");
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn test_build_command_script_with_trigger_args_windows_bat() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "win-bat-args".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.bat".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--flag"), None);
        let args = cmd.get_argv();

        // Should produce ["cmd.exe", "/C", "deploy.bat --flag"]
        assert_eq!(args.len(), 3, "Expected 3 args, got {:?}", args);
        assert_eq!(args[0].to_string_lossy(), "cmd.exe");
        assert_eq!(args[1].to_string_lossy(), "/C");
        assert_eq!(args[2].to_string_lossy(), "deploy.bat --flag");
    }

    #[tokio::test]
    async fn test_build_command_with_trigger_env() {
        let mut trigger_env = HashMap::new();
        trigger_env.insert("TRIGGER_VAR".to_string(), "trigger_value".to_string());

        let job = Job {
            id: Uuid::now_v7(),
            name: "env-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, None, Some(&trigger_env));
        // Verify the env was set by checking iter_extra_env_as_str
        let env_pairs: Vec<(String, String)> = cmd
            .iter_extra_env_as_str()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        assert!(
            env_pairs
                .iter()
                .any(|(k, v)| k == "TRIGGER_VAR" && v == "trigger_value"),
            "Trigger env should be set, got: {:?}",
            env_pairs
        );
    }

    #[tokio::test]
    async fn test_build_command_trigger_env_overrides_job_env() {
        let mut job_env = HashMap::new();
        job_env.insert("SHARED".to_string(), "job_value".to_string());

        let mut trigger_env = HashMap::new();
        trigger_env.insert("SHARED".to_string(), "trigger_value".to_string());

        let job = Job {
            id: Uuid::now_v7(),
            name: "override-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: Some(job_env),
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, None, Some(&trigger_env));
        // The last value set for SHARED should be "trigger_value"
        let env_pairs: Vec<(String, String)> = cmd
            .iter_extra_env_as_str()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        // Find all SHARED entries - the last one should be trigger_value
        let shared_values: Vec<&String> = env_pairs
            .iter()
            .filter(|(k, _)| k == "SHARED")
            .map(|(_, v)| v)
            .collect();
        assert!(!shared_values.is_empty(), "Should have SHARED env var");
        assert_eq!(
            shared_values.last().unwrap().as_str(),
            "trigger_value",
            "Trigger env should override job env"
        );
    }

    #[tokio::test]
    async fn test_spawn_job_uses_pregenerated_run_id() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"hello\n".to_vec()], 0);
        let (executor, _event_rx, _log_store) = setup_executor(spawner);
        let job = make_test_job();

        let expected_run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, expected_run_id, None)
            .await
            .expect("spawn_job");

        assert_eq!(
            handle.run_id, expected_run_id,
            "RunHandle should use the pre-generated run_id"
        );
        handle.join_handle.await.expect("join");
    }

    // --- Trigger params in log header and meta.json tests ---

    #[tokio::test]
    async fn test_log_header_includes_trigger_args_shell_command() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job(); // execution = ShellCommand("echo hello")

        let trigger_params = TriggerParams {
            args: Some("--verbose --flag".to_string()),
            env: None,
            input: None,
        };

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, Some(&trigger_params))
            .await
            .expect("spawn_job");
        let job_id = handle.job_id;
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let log_content = log_store
            .read_log(job_id, run_id, None)
            .await
            .expect("read_log");

        // The second line should be the effective command with trigger args appended
        // (first line is the JOB RUN START delimiter)
        let command_line = log_content
            .lines()
            .nth(1)
            .expect("should have at least two lines");
        assert!(
            command_line.contains("echo hello --verbose --flag"),
            "Log header should include trigger args. Got: {}",
            command_line
        );
        assert!(
            command_line.starts_with("$ "),
            "Log header should start with '$ '. Got: {}",
            command_line
        );
    }

    #[tokio::test]
    async fn test_log_header_without_trigger_args_shows_base_command() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job(); // execution = ShellCommand("echo hello")

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        let job_id = handle.job_id;
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let log_content = log_store
            .read_log(job_id, run_id, None)
            .await
            .expect("read_log");

        // The second line should be the base command (first line is the JOB RUN START delimiter)
        let command_line = log_content
            .lines()
            .nth(1)
            .expect("should have at least two lines");
        assert_eq!(
            command_line, "$ echo hello",
            "Log header should show base command without trigger args. Got: {}",
            command_line
        );
    }

    #[tokio::test]
    async fn test_log_header_includes_trigger_args_script_file() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let now = Utc::now();
        let job = Job {
            id: Uuid::now_v7(),
            name: "script-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.sh".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let trigger_params = TriggerParams {
            args: Some("--env prod".to_string()),
            env: None,
            input: None,
        };

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, Some(&trigger_params))
            .await
            .expect("spawn_job");
        let job_id = handle.job_id;
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let log_content = log_store
            .read_log(job_id, run_id, None)
            .await
            .expect("read_log");

        // The second line should be the script command header (first line is the JOB RUN START delimiter)
        let command_line = log_content
            .lines()
            .nth(1)
            .expect("should have at least two lines");
        assert!(
            command_line.contains("[script] deploy.sh --env prod"),
            "Log header should include trigger args for script file. Got: {}",
            command_line
        );
    }

    #[tokio::test]
    async fn test_job_run_stores_trigger_params() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let mut trigger_env = HashMap::new();
        trigger_env.insert("MY_VAR".to_string(), "my_value".to_string());

        let trigger_params = TriggerParams {
            args: Some("--flag".to_string()),
            env: Some(trigger_env),
            input: Some("stdin data".to_string()),
        };

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, Some(&trigger_params))
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Check that the run stored in the log store has trigger_params
        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert!(
            run.trigger_params.is_some(),
            "JobRun should have trigger_params stored"
        );
        let stored_params = run.trigger_params.as_ref().unwrap();
        assert_eq!(stored_params.args.as_deref(), Some("--flag"));
        assert_eq!(
            stored_params.env.as_ref().unwrap().get("MY_VAR").unwrap(),
            "my_value"
        );
        assert_eq!(stored_params.input.as_deref(), Some("stdin data"));
    }

    #[tokio::test]
    async fn test_job_run_no_trigger_params_when_none_provided() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert!(
            run.trigger_params.is_none(),
            "JobRun should not have trigger_params when none provided"
        );
    }

    #[tokio::test]
    async fn test_trigger_params_preserved_in_completed_run() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);
        let job = make_test_job();

        let trigger_params = TriggerParams {
            args: Some("--test".to_string()),
            env: None,
            input: None,
        };

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, Some(&trigger_params))
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // The final (updated) run should still have trigger_params
        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(run.status, RunStatus::Completed);
        assert!(
            run.trigger_params.is_some(),
            "Completed run should preserve trigger_params"
        );
        assert_eq!(
            run.trigger_params.as_ref().unwrap().args.as_deref(),
            Some("--test")
        );
    }

    #[tokio::test]
    async fn test_trigger_params_serialization_roundtrip() {
        let mut env = HashMap::new();
        env.insert("KEY".to_string(), "VALUE".to_string());

        let run = JobRun {
            run_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            status: RunStatus::Completed,
            exit_code: Some(0),
            log_size_bytes: 100,
            error: None,
            trigger_params: Some(TriggerParams {
                args: Some("--flag".to_string()),
                env: Some(env),
                input: Some("data".to_string()),
            }),
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        };

        let json = serde_json::to_string_pretty(&run).expect("serialize");
        let deserialized: JobRun = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(run, deserialized);
        assert!(deserialized.trigger_params.is_some());
        let tp = deserialized.trigger_params.unwrap();
        assert_eq!(tp.args.as_deref(), Some("--flag"));
        assert_eq!(tp.env.as_ref().unwrap().get("KEY").unwrap(), "VALUE");
        assert_eq!(tp.input.as_deref(), Some("data"));
    }

    #[tokio::test]
    async fn test_log_environment_includes_trigger_env_vars() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let now = Utc::now();
        let mut job_env = HashMap::new();
        job_env.insert("JOB_VAR".to_string(), "job_value".to_string());

        let job = Job {
            id: Uuid::now_v7(),
            name: "log-env-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: Some(job_env),
            timeout_secs: 0,
            log_environment: true,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            next_run_at: None,
        };

        let mut trigger_env = HashMap::new();
        trigger_env.insert("TRIGGER_VAR".to_string(), "trigger_value".to_string());

        let trigger_params = TriggerParams {
            args: None,
            env: Some(trigger_env),
            input: None,
        };

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, Some(&trigger_params))
            .await
            .expect("spawn_job");
        let job_id = handle.job_id;
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let log_content = log_store
            .read_log(job_id, run_id, None)
            .await
            .expect("read_log");

        // The environment dump should contain both job env vars and trigger env vars
        assert!(
            log_content.contains("=== Environment ==="),
            "Log should contain environment dump header. Got: {}",
            log_content
        );
        assert!(
            log_content.contains("JOB_VAR=job_value"),
            "Log should contain job env var. Got: {}",
            log_content
        );
        assert!(
            log_content.contains("TRIGGER_VAR=trigger_value"),
            "Log should contain trigger env var. Got: {}",
            log_content
        );
    }

    #[tokio::test]
    async fn test_trigger_params_none_not_in_json() {
        let run = JobRun {
            run_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            exit_code: None,
            log_size_bytes: 0,
            error: None,
            trigger_params: None,
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        };

        let json = serde_json::to_string(&run).expect("serialize");
        assert!(
            !json.contains("trigger_params"),
            "JSON should not contain trigger_params when None (skip_serializing_if). Got: {}",
            json
        );

        // Deserializing JSON without trigger_params should still work (serde default)
        let deserialized: JobRun = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.trigger_params.is_none());
    }

    // --- Hook integration tests ---

    fn setup_executor_with_config(
        spawner: MockPtySpawner,
        config: DaemonConfig,
    ) -> (
        Executor,
        broadcast::Receiver<JobEvent>,
        Arc<InMemoryLogStore>,
    ) {
        let config = Arc::new(config);
        let (event_tx, event_rx) = broadcast::channel::<JobEvent>(4096);
        let log_store = Arc::new(InMemoryLogStore::new());
        let pty_spawner = Arc::new(spawner);

        let executor = Executor::new(
            event_tx,
            Arc::clone(&log_store) as Arc<dyn LogStore>,
            config,
            pty_spawner as Arc<dyn PtySpawner>,
        );

        (executor, event_rx, log_store)
    }

    #[tokio::test]
    async fn test_pre_hook_success_allows_job_to_run() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"job output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.pre_hook = Some("echo pre-hook-ran".to_string());

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should complete normally when pre-hook succeeds"
        );
        assert!(run.error.is_none(), "Should have no error on success");
    }

    #[tokio::test]
    async fn test_pre_hook_failure_blocks_job() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"job output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.pre_hook = Some("exit 1".to_string());

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Failed,
            "Job should fail when pre-hook fails"
        );
        assert!(run.error.is_some(), "Should have an error message");
        let error = run.error.as_ref().unwrap();
        assert!(
            error.to_lowercase().contains("pre-hook"),
            "Error should mention pre-hook, got: {}",
            error
        );
        // Verify the job itself did NOT execute: exit_code is None (pre-hook failure path
        // sets exit_code to None in the stored run)
        assert!(
            run.exit_code.is_none(),
            "Exit code should be None when pre-hook blocked the job"
        );
    }

    #[tokio::test]
    async fn test_post_hook_failure_yields_completed_with_warnings() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"job output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.post_hook = Some("exit 1".to_string());

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::CompletedWithWarnings,
            "Job should be CompletedWithWarnings when post-hook fails"
        );
        assert!(run.error.is_some(), "Should have an error message");
        let error = run.error.as_ref().unwrap();
        assert!(
            error.to_lowercase().contains("post-hook"),
            "Error should mention post-hook, got: {}",
            error
        );
        // The main job succeeded (exit code 0)
        assert_eq!(
            run.exit_code,
            Some(0),
            "Exit code should reflect the main job result"
        );
    }

    #[tokio::test]
    async fn test_post_hook_success_yields_completed() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"job output\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.post_hook = Some("echo post-hook-ran".to_string());

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should be Completed when post-hook succeeds"
        );
        assert!(run.error.is_none(), "Should have no error on success");
        assert_eq!(run.exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_hook_inherited_from_config_default_pre_hook() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"job output\n".to_vec()], 0);
        let config = DaemonConfig {
            default_pre_hook: Some("echo global-pre-hook".to_string()),
            ..Default::default()
        };
        let (executor, _event_rx, log_store) = setup_executor_with_config(spawner, config);

        // Job has no hook set — should inherit from config
        let job = make_test_job();

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should complete normally when inherited config pre-hook succeeds"
        );
        assert!(run.error.is_none());
    }

    #[tokio::test]
    async fn test_job_level_hook_overrides_config_default() {
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"job output\n".to_vec()], 0);
        // Config pre-hook would fail if it ran
        let config = DaemonConfig {
            default_pre_hook: Some("exit 1".to_string()),
            ..Default::default()
        };
        let (executor, _event_rx, log_store) = setup_executor_with_config(spawner, config);

        // Job-level pre-hook succeeds — should override the failing config hook
        let mut job = make_test_job();
        job.pre_hook = Some("echo override-hook".to_string());

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should succeed when job-level hook overrides the failing config default"
        );
        assert!(
            run.error.is_none(),
            "Should have no error when job-level hook overrides config"
        );
    }

    #[tokio::test]
    async fn test_build_command_trigger_args_newline_sanitized() {
        let now = Utc::now();
        let job = Job {
            id: Uuid::now_v7(),
            name: "newline-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--flag\n--other"), None);
        let args = cmd.get_argv();

        // The trigger args newline should be replaced with a space in the final command string
        let full_cmd = args[2].to_string_lossy();
        assert!(
            !full_cmd.contains('\n'),
            "Command should not contain newline. Got: {:?}",
            full_cmd
        );
        assert!(
            full_cmd.contains("--flag --other"),
            "Newline should be replaced with space. Got: {:?}",
            full_cmd
        );
    }

    #[tokio::test]
    async fn test_build_command_trigger_args_crlf_sanitized() {
        let now = Utc::now();
        let job = Job {
            id: Uuid::now_v7(),
            name: "crlf-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--flag\r\n--other"), None);
        let args = cmd.get_argv();

        // The trigger args CRLF should be replaced with spaces in the final command string
        let full_cmd = args[2].to_string_lossy();
        assert!(
            !full_cmd.contains('\n'),
            "Command should not contain newline. Got: {:?}",
            full_cmd
        );
        assert!(
            !full_cmd.contains('\r'),
            "Command should not contain carriage return. Got: {:?}",
            full_cmd
        );
        assert!(
            full_cmd.contains("--flag") && full_cmd.contains("--other"),
            "Both flags should be present. Got: {:?}",
            full_cmd
        );
    }

    #[tokio::test]
    async fn test_build_command_script_trigger_args_newline_sanitized() {
        let now = Utc::now();
        let job = Job {
            id: Uuid::now_v7(),
            name: "script-newline-test".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ScriptFile("deploy.sh".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            allow_concurrent: false,
            schedule_mode: crate::models::ScheduleMode::default(),
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job, Some("--env\nprod"), None);
        let args = cmd.get_argv();

        // The final command argument (index 1 on unix with trigger args uses -c, index 2 is the cmd string)
        // On Windows it's args[2] as well. Either way, no newlines should appear.
        let full_cmd = args.last().unwrap().to_string_lossy();
        assert!(
            !full_cmd.contains('\n'),
            "Script command should not contain newline. Got: {:?}",
            full_cmd
        );
        assert!(
            full_cmd.contains("--env") && full_cmd.contains("prod"),
            "Both parts of args should be present. Got: {:?}",
            full_cmd
        );
    }

    // --- extract_cost_from_log unit tests ---

    #[test]
    fn test_extract_cost_full_ndjson_stream() {
        let log = concat!(
            r#"{"type":"system","subtype":"init","session_id":"abc123","tools":["Read","Write"],"model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.0342,"duration_ms":15234,"num_turns":3,"usage":{"input_tokens":1234,"output_tokens":567,"cache_creation_input_tokens":100,"cache_read_input_tokens":200}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        assert_eq!(summary.total_cost_usd, Some(0.0342));
        assert_eq!(summary.duration_ms, Some(15234));
        assert_eq!(summary.num_turns, Some(3));
        assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-20250514"));

        let usage = summary.usage.expect("usage should be Some");
        assert_eq!(usage["input_tokens"], 1234);
        assert_eq!(usage["output_tokens"], 567);
        assert_eq!(usage["cache_creation_input_tokens"], 100);
        assert_eq!(usage["cache_read_input_tokens"], 200);
    }

    #[test]
    fn test_extract_cost_empty_content() {
        let summary = extract_cost_from_log(b"");
        assert!(summary.total_cost_usd.is_none());
        assert!(summary.duration_ms.is_none());
        assert!(summary.num_turns.is_none());
        assert!(summary.model.is_none());
        assert!(summary.usage.is_none());
    }

    #[test]
    fn test_extract_cost_non_ndjson_plain_shell_output() {
        let log = b"$ echo hello\nhello\n";
        let summary = extract_cost_from_log(log);
        assert!(summary.total_cost_usd.is_none());
        assert!(summary.duration_ms.is_none());
        assert!(summary.num_turns.is_none());
        assert!(summary.model.is_none());
        assert!(summary.usage.is_none());
    }

    #[test]
    fn test_extract_cost_result_without_system_event() {
        let log = concat!(
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.01,"duration_ms":5000,"num_turns":1,"usage":{"input_tokens":100,"output_tokens":50}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());
        assert_eq!(summary.total_cost_usd, Some(0.01));
        assert_eq!(summary.duration_ms, Some(5000));
        assert_eq!(summary.num_turns, Some(1));
        assert!(
            summary.model.is_none(),
            "model should be None when no system event"
        );
        assert!(summary.usage.is_some());
    }

    #[test]
    fn test_extract_cost_zero_cost() {
        let log = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-haiku-4"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.0,"duration_ms":100,"num_turns":1,"usage":{}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());
        assert_eq!(summary.total_cost_usd, Some(0.0));
        assert_eq!(summary.model.as_deref(), Some("claude-haiku-4"));
    }

    #[test]
    fn test_extract_cost_large_log_full_scan() {
        // Build a log larger than 8KB with filler between system and result events to
        // verify that the full-scan approach finds both regardless of their position.
        let system_line = r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#
            .to_string() + "\n";
        let filler: String = (0..500)
            .map(|i| format!("plain output line {}\n", i))
            .collect();
        let result_line = r#"{"type":"result","total_cost_usd":1.23,"duration_ms":99999,"num_turns":7,"usage":{"input_tokens":9999,"output_tokens":9999}}"#.to_string() + "\n";

        let log = format!("{}{}{}", system_line, filler, result_line);
        let bytes = log.as_bytes();

        // Confirm log is indeed larger than 8KB so this is a meaningful regression guard.
        assert!(
            bytes.len() > 8192,
            "Log should be > 8KB for this test, got {} bytes",
            bytes.len()
        );

        let summary = extract_cost_from_log(bytes);
        assert_eq!(summary.total_cost_usd, Some(1.23));
        assert_eq!(summary.duration_ms, Some(99999));
        assert_eq!(summary.num_turns, Some(7));
        assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-20250514"));
    }

    #[test]
    fn test_extract_cost_two_sequential_invocations() {
        // Two complete Claude NDJSON blocks back-to-back in the same log.
        let log = concat!(
            // First invocation
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"First"}]}}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.05,"duration_ms":1000,"num_turns":3,"usage":{"input_tokens":100,"output_tokens":50}}"#,
            "\n",
            // Second invocation
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Second"}]}}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.08,"duration_ms":2000,"num_turns":5,"usage":{"input_tokens":200,"output_tokens":80}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        // Costs should be summed across both invocations.
        assert!(
            (summary.total_cost_usd.unwrap() - 0.13).abs() < 1e-10,
            "expected total_cost_usd ~0.13, got {:?}",
            summary.total_cost_usd
        );
        assert_eq!(summary.duration_ms, Some(3000));
        assert_eq!(summary.num_turns, Some(8));
        // Model comes from the first system event.
        assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-20250514"));

        let usage = summary.usage.expect("usage should be Some");
        assert_eq!(usage["input_tokens"], 300);
        assert_eq!(usage["output_tokens"], 130);
    }

    #[test]
    fn test_extract_cost_mixed_shell_and_claude_output() {
        // Shell output lines surrounding two Claude NDJSON blocks.
        let log = concat!(
            "Starting backup...\n",
            "Connecting to remote host...\n",
            // First Claude block
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.03,"duration_ms":500,"num_turns":2,"usage":{"input_tokens":60,"output_tokens":20}}"#,
            "\n",
            "Backup complete.\n",
            "Verifying checksums...\n",
            // Second Claude block
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.07,"duration_ms":1500,"num_turns":4,"usage":{"input_tokens":140,"output_tokens":60}}"#,
            "\n",
            "All done.\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        // Shell output lines must be silently ignored; costs from both Claude blocks summed.
        assert!(
            (summary.total_cost_usd.unwrap() - 0.10).abs() < 1e-10,
            "expected total_cost_usd ~0.10, got {:?}",
            summary.total_cost_usd
        );
        assert_eq!(summary.duration_ms, Some(2000));
        assert_eq!(summary.num_turns, Some(6));

        let usage = summary.usage.expect("usage should be Some");
        assert_eq!(usage["input_tokens"], 200);
        assert_eq!(usage["output_tokens"], 80);
    }

    #[test]
    fn test_extract_cost_large_env_dump_before_system_event() {
        // Simulate a 6KB+ environment variable dump that precedes the Claude NDJSON block.
        // This was the old head-window bug: the system event was beyond the first ~4KB window
        // so the model was never extracted. The full-scan approach must handle it correctly.
        let env_dump: String = (0..200)
            .map(|i| format!("ENV_VAR_NUMBER_{}=some_value_that_is_quite_long_{}\n", i, i))
            .collect();
        let claude_block = concat!(
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.05,"duration_ms":2000,"num_turns":2,"usage":{"input_tokens":100,"output_tokens":40}}"#,
            "\n"
        );

        let log = format!("{}{}", env_dump, claude_block);
        let bytes = log.as_bytes();

        // Confirm the env dump is genuinely large enough to trigger the old bug.
        assert!(
            bytes.len() > 6144,
            "env dump should be > 6KB for this test, got {} bytes",
            bytes.len()
        );

        let summary = extract_cost_from_log(bytes);
        assert_eq!(summary.total_cost_usd, Some(0.05));
        // Model must be found even though it appears far into the log.
        assert_eq!(
            summary.model.as_deref(),
            Some("claude-sonnet-4-20250514"),
            "model should be extracted correctly despite large env dump prefix"
        );
    }

    #[test]
    fn test_extract_cost_hook_output_appended_to_main_log() {
        // Simulates a post-hook whose NDJSON output is appended to the main command log,
        // separated by a plain-text comment line.
        let log = concat!(
            // Main command Claude block
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.04,"duration_ms":800,"num_turns":2,"usage":{"input_tokens":80,"output_tokens":30}}"#,
            "\n",
            // Plain separator (not JSON)
            "--- post-hook ---\n",
            // Post-hook Claude block
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.02,"duration_ms":400,"num_turns":1,"usage":{"input_tokens":40,"output_tokens":15}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        // Costs from both blocks must be summed; separator line must be ignored.
        assert!(
            (summary.total_cost_usd.unwrap() - 0.06).abs() < 1e-10,
            "expected total_cost_usd ~0.06, got {:?}",
            summary.total_cost_usd
        );
        assert_eq!(summary.duration_ms, Some(1200));
        assert_eq!(summary.num_turns, Some(3));

        let usage = summary.usage.expect("usage should be Some");
        assert_eq!(usage["input_tokens"], 120);
        assert_eq!(usage["output_tokens"], 45);
    }

    #[test]
    fn test_extract_cost_prehook_plus_main_summed() {
        // Simulates a pre-hook Claude invocation followed by the main Claude command,
        // separated by a PTY header / plain-text line. Costs must be summed.
        let log = concat!(
            // Pre-hook Claude block
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.05,"duration_ms":2000,"num_turns":2,"usage":{"input_tokens":5000,"output_tokens":1000}}"#,
            "\n",
            // PTY header / separator (not JSON)
            "--- main command ---\n",
            // Main command Claude block
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.15,"duration_ms":6000,"num_turns":5,"usage":{"input_tokens":15000,"output_tokens":4000}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        // Costs from pre-hook and main command must be summed.
        assert!(
            (summary.total_cost_usd.unwrap() - 0.20).abs() < 1e-10,
            "expected total_cost_usd ~0.20, got {:?}",
            summary.total_cost_usd
        );
        // Model comes from the first system event (pre-hook).
        assert_eq!(
            summary.model.as_deref(),
            Some("claude-sonnet-4-20250514"),
            "model should be taken from the first system event"
        );

        let usage = summary.usage.expect("usage should be Some");
        assert_eq!(usage["input_tokens"], 20000);
        assert_eq!(usage["output_tokens"], 5000);
    }

    #[test]
    fn test_extract_cost_different_models_across_invocations() {
        // When two invocations use different models, the model field should contain
        // only the FIRST model seen (from the first system event).
        let log = concat!(
            // First invocation — sonnet
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.05,"duration_ms":1000,"num_turns":2,"usage":{"input_tokens":100,"output_tokens":40}}"#,
            "\n",
            // Second invocation — opus
            r#"{"type":"system","subtype":"init","model":"claude-opus-4-20250514"}"#,
            "\n",
            r#"{"type":"result","total_cost_usd":0.20,"duration_ms":3000,"num_turns":4,"usage":{"input_tokens":400,"output_tokens":160}}"#,
            "\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        // Costs are still summed across both invocations.
        assert!(
            (summary.total_cost_usd.unwrap() - 0.25).abs() < 1e-10,
            "expected total_cost_usd ~0.25, got {:?}",
            summary.total_cost_usd
        );
        // Model must be the FIRST one encountered.
        assert_eq!(
            summary.model.as_deref(),
            Some("claude-sonnet-4-20250514"),
            "model should be taken from the first system event"
        );
    }

    #[test]
    fn test_extract_cost_piped_input_with_csv_data() {
        // Simulate piped-input Claude invocation: cat data.csv | claude -p "summarize"
        // The log contains CSV data lines, shell output, and Claude NDJSON events.
        let log = concat!(
            // CSV data (simulating piped input)
            "name,age,city\n",
            "Alice,30,NYC\n",
            "Bob,25,LA\n",
            // Shell output lines
            "Processing 3 rows...\n",
            // Claude NDJSON system event
            r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#,
            "\n",
            // Claude NDJSON result event with cost and usage
            r#"{"type":"result","total_cost_usd":0.0123,"duration_ms":850,"num_turns":1,"usage":{"input_tokens":156,"output_tokens":42}}"#,
            "\n",
            // More shell output
            "Summary complete.\n"
        );
        let summary = extract_cost_from_log(log.as_bytes());

        // CSV and shell output lines must be silently ignored.
        // Cost, model, duration, and usage must be correctly extracted.
        assert!(
            (summary.total_cost_usd.unwrap() - 0.0123).abs() < 1e-10,
            "expected total_cost_usd ~0.0123, got {:?}",
            summary.total_cost_usd
        );
        assert_eq!(summary.duration_ms, Some(850));
        assert_eq!(summary.num_turns, Some(1));
        assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-20250514"));

        let usage = summary.usage.expect("usage should be Some");
        assert_eq!(usage["input_tokens"], 156);
        assert_eq!(usage["output_tokens"], 42);
    }

    // --- Hook cost capture integration tests ---

    /// Write NDJSON content to a temp file and return a shell command that cats it.
    /// Works cross-platform (type on Windows, cat on Unix).
    fn ndjson_hook_command(dir: &std::path::Path, filename: &str, content: &str) -> String {
        let file_path = dir.join(filename);
        std::fs::write(&file_path, content).expect("write ndjson file");
        let path_str = file_path.to_string_lossy().to_string();
        if cfg!(target_os = "windows") {
            // cmd.exe /C passes the command as a single argument; avoid extra
            // quotes around the path (temp paths never contain spaces).
            format!("type {}", path_str)
        } else {
            format!("cat \"{}\"", path_str)
        }
    }

    #[tokio::test]
    async fn test_pre_hook_claude_cost_captured() {
        let tmp_dir = tempfile::tempdir().expect("create temp dir");

        let ndjson = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-sonnet-4-20250514\"}\n",
            "{\"type\":\"result\",\"total_cost_usd\":0.05,\"duration_ms\":2000,\"num_turns\":2,\"usage\":{\"input_tokens\":500,\"output_tokens\":200}}\n"
        );
        let hook_cmd = ndjson_hook_command(tmp_dir.path(), "pre_hook_ndjson.txt", ndjson);

        // Main command produces plain output (no Claude NDJSON)
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"done\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.pre_hook = Some(hook_cmd);

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should complete successfully"
        );
        assert_eq!(
            run.total_cost_usd,
            Some(0.05),
            "Pre-hook cost should be captured. Run: {:?}",
            run
        );
        assert_eq!(run.duration_ms, Some(2000));
        assert_eq!(run.num_turns, Some(2));
        assert_eq!(
            run.model.as_deref(),
            Some("claude-sonnet-4-20250514"),
            "Model should be captured from pre-hook NDJSON"
        );
        assert!(
            run.usage.is_some(),
            "Usage should be captured from pre-hook"
        );
    }

    #[tokio::test]
    async fn test_post_hook_claude_cost_captured() {
        let tmp_dir = tempfile::tempdir().expect("create temp dir");

        let ndjson = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-sonnet-4-20250514\"}\n",
            "{\"type\":\"result\",\"total_cost_usd\":0.08,\"duration_ms\":3000,\"num_turns\":4,\"usage\":{\"input_tokens\":800,\"output_tokens\":300}}\n"
        );
        let hook_cmd = ndjson_hook_command(tmp_dir.path(), "post_hook_ndjson.txt", ndjson);

        // Main command produces plain output (no Claude NDJSON)
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"done\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.post_hook = Some(hook_cmd);

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should complete successfully"
        );
        // Post-hook output triggers re-extraction, so cost should be present
        assert_eq!(
            run.total_cost_usd,
            Some(0.08),
            "Post-hook cost should be captured via re-extraction. Run: {:?}",
            run
        );
        assert_eq!(run.duration_ms, Some(3000));
        assert_eq!(run.num_turns, Some(4));
        assert_eq!(
            run.model.as_deref(),
            Some("claude-sonnet-4-20250514"),
            "Model should be captured from post-hook NDJSON"
        );
        assert!(
            run.usage.is_some(),
            "Usage should be captured from post-hook"
        );
    }

    #[tokio::test]
    async fn test_main_and_post_hook_costs_summed() {
        let tmp_dir = tempfile::tempdir().expect("create temp dir");

        // Post-hook NDJSON (cost = 0.03)
        let post_ndjson = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-sonnet-4-20250514\"}\n",
            "{\"type\":\"result\",\"total_cost_usd\":0.03,\"duration_ms\":1500,\"num_turns\":1,\"usage\":{\"input_tokens\":200,\"output_tokens\":100}}\n"
        );
        let hook_cmd = ndjson_hook_command(tmp_dir.path(), "post_hook_sum.txt", post_ndjson);

        // Main command produces Claude NDJSON via the mock PTY (cost = 0.07)
        let main_ndjson_system =
            b"{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-sonnet-4-20250514\"}\n"
                .to_vec();
        let main_ndjson_result =
            b"{\"type\":\"result\",\"total_cost_usd\":0.07,\"duration_ms\":5000,\"num_turns\":3,\"usage\":{\"input_tokens\":600,\"output_tokens\":400}}\n"
                .to_vec();

        let spawner =
            MockPtySpawner::with_output_and_exit(vec![main_ndjson_system, main_ndjson_result], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.post_hook = Some(hook_cmd);

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        assert_eq!(
            run.status,
            RunStatus::Completed,
            "Job should complete successfully"
        );
        // Costs should be summed: 0.07 (main) + 0.03 (post-hook) = 0.10
        let total = run.total_cost_usd.expect("total_cost_usd should be Some");
        assert!(
            (total - 0.10).abs() < 1e-9,
            "Costs should be summed: expected 0.10, got {}",
            total
        );
        // duration_ms should be summed: 5000 + 1500 = 6500
        assert_eq!(
            run.duration_ms,
            Some(6500),
            "Duration ms should be summed from main and post-hook"
        );
        // num_turns should be summed: 3 + 1 = 4
        assert_eq!(
            run.num_turns,
            Some(4),
            "Num turns should be summed from main and post-hook"
        );
        // Usage tokens should be summed
        let usage = run.usage.as_ref().expect("usage should be Some");
        assert_eq!(
            usage["input_tokens"], 800,
            "Input tokens should be summed: 600 + 200"
        );
        assert_eq!(
            usage["output_tokens"], 500,
            "Output tokens should be summed: 400 + 100"
        );
    }

    #[tokio::test]
    async fn test_posthook_failure_still_captures_cost() {
        let tmp_dir = tempfile::tempdir().expect("create temp dir");

        let ndjson = concat!(
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"claude-sonnet-4-20250514\"}\n",
            "{\"type\":\"result\",\"total_cost_usd\":0.04,\"duration_ms\":1200,\"num_turns\":2,\"usage\":{\"input_tokens\":300,\"output_tokens\":150}}\n"
        );

        // Build a hook command that emits NDJSON to stdout and then exits with code 1.
        // run_hook wraps this with `cmd /C` (Windows) or `sh -c` (Unix), so inline
        // shell syntax works on both platforms.
        let file_path = tmp_dir.path().join("failing_hook_ndjson.txt");
        std::fs::write(&file_path, ndjson).expect("write ndjson file");
        let path_str = file_path.to_string_lossy().to_string();
        let hook_cmd = if cfg!(target_os = "windows") {
            // cmd.exe: type prints the file, then `& exit /b 1` sets exit code 1
            format!("type {} & exit /b 1", path_str)
        } else {
            // sh: cat prints the file, then `; exit 1` sets exit code 1
            format!("cat \"{}\"; exit 1", path_str)
        };

        // Main command produces plain output (no NDJSON)
        let spawner = MockPtySpawner::with_output_and_exit(vec![b"main done\n".to_vec()], 0);
        let (executor, _event_rx, log_store) = setup_executor(spawner);

        let mut job = make_test_job();
        job.post_hook = Some(hook_cmd);

        let run_id = Uuid::now_v7();
        let handle = executor
            .spawn_job(&job, run_id, None)
            .await
            .expect("spawn_job");
        handle.join_handle.await.expect("join");

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run should exist");

        // Post-hook failure → CompletedWithWarnings, not Failed
        assert_eq!(
            run.status,
            RunStatus::CompletedWithWarnings,
            "Job should be CompletedWithWarnings when post-hook fails"
        );

        // Even though the post-hook failed, its stdout (NDJSON) was captured and
        // cost fields should still be extracted.
        assert_eq!(
            run.total_cost_usd,
            Some(0.04),
            "Cost should be captured from failing post-hook stdout. Run: {:?}",
            run
        );
        assert_eq!(
            run.duration_ms,
            Some(1200),
            "Duration should be captured from failing post-hook stdout"
        );
        assert_eq!(
            run.num_turns,
            Some(2),
            "num_turns should be captured from failing post-hook stdout"
        );
        assert_eq!(
            run.model.as_deref(),
            Some("claude-sonnet-4-20250514"),
            "Model should be captured from failing post-hook NDJSON"
        );
        assert!(
            run.usage.is_some(),
            "Usage should be captured from failing post-hook stdout"
        );
        let usage = run.usage.as_ref().unwrap();
        assert_eq!(usage["input_tokens"], 300);
        assert_eq!(usage["output_tokens"], 150);
    }
}
