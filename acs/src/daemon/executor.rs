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
    /// Hook succeeded (exit code zero).
    Success,
    /// Hook failed: carries a human-readable description of the failure.
    Failure(String),
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
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/C").arg(command);
        c
    } else {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
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
            return HookOutcome::Failure(format!("{} spawn error: {}", label, e));
        }
    };

    // Apply a 30-second timeout
    match tokio::time::timeout(std::time::Duration::from_secs(30), child.wait_with_output()).await {
        Ok(Ok(output)) => {
            if output.status.success() {
                HookOutcome::Success
            } else {
                let code = output.status.code().unwrap_or(-1);
                let stderr = String::from_utf8_lossy(&output.stderr);
                let detail = if stderr.trim().is_empty() {
                    format!("exited with code {}", code)
                } else {
                    format!("exited with code {}: {}", code, stderr.trim())
                };
                HookOutcome::Failure(format!("{} {}", label, detail))
            }
        }
        Ok(Err(e)) => HookOutcome::Failure(format!("{} wait error: {}", label, e)),
        Err(_) => HookOutcome::Failure(format!("{} timed out after 30 seconds", label)),
    }
}

/// Extracted cost/usage summary from a Claude CLI NDJSON log.
struct CostSummary {
    total_cost_usd: Option<f64>,
    duration_ms: Option<u64>,
    num_turns: Option<u32>,
    model: Option<String>,
    usage: Option<serde_json::Value>,
}

/// Extract cost data from a Claude CLI NDJSON log.
///
/// Claude CLI emits newline-delimited JSON events. The relevant events are:
/// - `{"type":"system","subtype":"init",...,"model":"<model>"}` — always near the start
/// - `{"type":"result",...,"total_cost_usd":...}` — always the last line
///
/// For performance, only the first 4 KB and last 8 KB of the log are scanned.
/// Non-NDJSON content (plain shell output, etc.) results in all fields being `None`.
fn extract_cost_from_log(log_content: &[u8]) -> CostSummary {
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

    // Scan the first ~4KB for the system event (model info is at the start)
    let head_window = &log_content[..log_content.len().min(4096)];
    let head_str = String::from_utf8_lossy(head_window);
    for line in head_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("system") {
                if let Some(model) = val.get("model").and_then(|m| m.as_str()) {
                    summary.model = Some(model.to_string());
                }
                break;
            }
        }
    }

    // Scan the last ~8KB for the result event (always the final NDJSON line)
    let tail_start = log_content.len().saturating_sub(8192);
    let tail_window = &log_content[tail_start..];
    let tail_str = String::from_utf8_lossy(tail_window);
    for line in tail_str.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("result") {
                if let Some(cost) = val.get("total_cost_usd").and_then(|v| v.as_f64()) {
                    summary.total_cost_usd = Some(cost);
                }
                if let Some(ms) = val.get("duration_ms").and_then(|v| v.as_u64()) {
                    summary.duration_ms = Some(ms);
                }
                if let Some(turns) = val.get("num_turns").and_then(|v| v.as_u64()) {
                    summary.num_turns = Some(turns as u32);
                }
                if let Some(usage) = val.get("usage").cloned() {
                    summary.usage = Some(usage);
                }
                break;
            }
        }
    }

    summary
}

/// Handle to a running job, allowing monitoring and cancellation.
pub struct RunHandle {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub join_handle: tokio::task::JoinHandle<()>,
    pub kill_tx: oneshot::Sender<()>,
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
        let (kill_tx, kill_rx) = oneshot::channel::<()>();

        // Spawn the execution task
        let join_handle = tokio::spawn(async move {
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
                    HookOutcome::Success => {
                        tracing::debug!("Pre-hook succeeded for job {}", job_id);
                    }
                    HookOutcome::Failure(detail) => {
                        let error_msg = format!("Pre-hook failed: {}", detail);
                        tracing::warn!("{} for job {}", error_msg, job_id);

                        let failed_run = JobRun {
                            run_id,
                            job_id,
                            started_at: now,
                            finished_at: Some(Utc::now()),
                            status: RunStatus::Failed,
                            exit_code: None,
                            log_size_bytes: 0,
                            error: Some(error_msg.clone()),
                            trigger_params: trigger_params_owned.clone(),
                            total_cost_usd: None,
                            duration_ms: None,
                            num_turns: None,
                            model: None,
                            usage: None,
                        };
                        if let Err(e) = log_store.update_run(&failed_run).await {
                            tracing::error!("Failed to save run on pre-hook failure: {}", e);
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
                    _ = &mut kill_rx => {
                        killed = true;
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
            let total_bytes: u64 = (log_writer_handle.await).unwrap_or_default();

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
                // Job was killed
                let killed_run = JobRun {
                    run_id,
                    job_id,
                    started_at: now,
                    finished_at: Some(finished_at),
                    status: RunStatus::Killed,
                    exit_code: None,
                    log_size_bytes: total_bytes,
                    error: Some("Job was killed".to_string()),
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
                let _ = event_tx.send(JobEvent::Failed {
                    job_id,
                    run_id,
                    error: "Job was killed".to_string(),
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
                            HookOutcome::Success => {
                                tracing::debug!("Post-hook succeeded for job {}", job_id);
                                (RunStatus::Completed, None)
                            }
                            HookOutcome::Failure(detail) => {
                                let error_msg = format!("Post-hook failed: {}", detail);
                                tracing::warn!("{} for job {}", error_msg, job_id);
                                (RunStatus::CompletedWithWarnings, Some(error_msg))
                            }
                        }
                    } else {
                        (RunStatus::Completed, None)
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
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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

        // Count Output events (includes 1 command header + 3 chunks = 4)
        let output_count = events
            .iter()
            .filter(|e| matches!(e, JobEvent::Output { .. }))
            .count();

        assert_eq!(
            output_count, 4,
            "Expected 4 Output events (1 header + 3 chunks), got {}",
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

        // Should have Started, command header Output, and Completed events
        let output_count = events
            .iter()
            .filter(|e| matches!(e, JobEvent::Output { .. }))
            .count();
        assert_eq!(
            output_count, 1,
            "Should have only the command header Output event"
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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

        // The first line should be the effective command with trigger args appended
        let first_line = log_content
            .lines()
            .next()
            .expect("should have at least one line");
        assert!(
            first_line.contains("echo hello --verbose --flag"),
            "Log header should include trigger args. Got: {}",
            first_line
        );
        assert!(
            first_line.starts_with("$ "),
            "Log header should start with '$ '. Got: {}",
            first_line
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

        let first_line = log_content
            .lines()
            .next()
            .expect("should have at least one line");
        assert_eq!(
            first_line, "$ echo hello",
            "Log header should show base command without trigger args. Got: {}",
            first_line
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
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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

        let first_line = log_content
            .lines()
            .next()
            .expect("should have at least one line");
        assert!(
            first_line.contains("[script] deploy.sh --env prod"),
            "Log header should include trigger args for script file. Got: {}",
            first_line
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
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            pre_hook: None,
            post_hook: None,
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
            allow_concurrent: false,
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
            allow_concurrent: false,
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
            allow_concurrent: false,
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
    fn test_extract_cost_large_log_uses_tail_window() {
        // Build a log that is larger than 8KB, with the result event at the very end
        let system_line = r#"{"type":"system","subtype":"init","model":"claude-sonnet-4-20250514"}"#
            .to_string() + "\n";
        // Fill with non-NDJSON lines to push the result event past the 8KB head/tail boundary
        let filler: String = (0..500)
            .map(|i| format!("plain output line {}\n", i))
            .collect();
        let result_line = r#"{"type":"result","total_cost_usd":1.23,"duration_ms":99999,"num_turns":7,"usage":{"input_tokens":9999,"output_tokens":9999}}"#.to_string() + "\n";

        let log = format!("{}{}{}", system_line, filler, result_line);
        let bytes = log.as_bytes();

        // Confirm log is indeed larger than 8KB
        assert!(
            bytes.len() > 8192,
            "Log should be > 8KB for this test, got {} bytes",
            bytes.len()
        );

        let summary = extract_cost_from_log(bytes);
        assert_eq!(summary.total_cost_usd, Some(1.23));
        assert_eq!(summary.duration_ms, Some(99999));
        assert_eq!(summary.num_turns, Some(7));
        // model is in the first 4KB (system line is small) so it should be found
        assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-20250514"));
    }
}
