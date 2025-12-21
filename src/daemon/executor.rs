use std::sync::Arc;

use chrono::Utc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tracing;
use uuid::Uuid;

use crate::daemon::events::JobEvent;
use crate::models::{DaemonConfig, ExecutionType, Job, JobRun, RunStatus};
use crate::pty::PtySpawner;
use crate::storage::LogStore;

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
    fn build_command(job: &Job) -> portable_pty::CommandBuilder {
        let mut cmd = match &job.execution {
            ExecutionType::ShellCommand(command) => {
                if cfg!(target_os = "windows") {
                    let mut cb = portable_pty::CommandBuilder::new("cmd.exe");
                    cb.arg("/C");
                    cb.arg(command);
                    cb
                } else {
                    let mut cb = portable_pty::CommandBuilder::new("/bin/sh");
                    cb.arg("-c");
                    cb.arg(command);
                    cb
                }
            }
            ExecutionType::ScriptFile(script) => {
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
                            cb.arg("-File");
                            cb.arg(script);
                            cb
                        }
                        _ => {
                            let mut cb = portable_pty::CommandBuilder::new("cmd.exe");
                            cb.arg("/C");
                            cb.arg(script);
                            cb
                        }
                    }
                } else {
                    let mut cb = portable_pty::CommandBuilder::new("/bin/sh");
                    cb.arg(script);
                    cb
                }
            }
        };

        // Set working directory if specified
        if let Some(ref dir) = job.working_dir {
            cmd.cwd(dir);
        }

        // Set environment variables if specified
        if let Some(ref env_vars) = job.env_vars {
            for (key, value) in env_vars {
                cmd.env(key, value);
            }
        }

        cmd
    }

    /// Spawn a job, returning a RunHandle for monitoring and cancellation.
    pub async fn spawn_job(&self, job: &Job) -> anyhow::Result<RunHandle> {
        let run_id = Uuid::now_v7();
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

        // Build the command
        let cmd = Self::build_command(job);

        // Clone things for the spawned task
        let execution = job.execution.clone();
        let log_environment = job.log_environment;
        let job_env_vars = job.env_vars.clone();
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

            // If log_environment is enabled, dump full environment before command
            if log_environment {
                let mut env_map: std::collections::BTreeMap<String, String> = std::env::vars().collect();
                // Merge job-specific env vars (these override inherited ones)
                if let Some(ref job_envs) = job_env_vars {
                    for (k, v) in job_envs {
                        env_map.insert(k.clone(), v.clone());
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

            // Write command header to log
            let command_str = match &execution {
                ExecutionType::ShellCommand(cmd) => cmd.clone(),
                ExecutionType::ScriptFile(script) => format!("[script] {}", script),
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

                    // Per SPEC: non-zero exit is Completed (not Failed).
                    // Failed = infrastructure error only.
                    let completed_run = JobRun {
                        run_id,
                        job_id,
                        started_at: now,
                        finished_at: Some(finished_at),
                        status: RunStatus::Completed,
                        exit_code: Some(exit_code),
                        log_size_bytes: total_bytes,
                        error: None,
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
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");

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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job);
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };

        let cmd = Executor::build_command(&job);
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
        let mut config = DaemonConfig::default();
        config.default_timeout_secs = timeout_secs;
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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
        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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

        let handle = executor.spawn_job(&job).await.expect("spawn_job");
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
}
