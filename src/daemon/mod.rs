// Daemon module - Phase 2+ implementation
// Sub-modules for events, executor, scheduler, and service.

pub mod events;
pub mod executor;
pub mod scheduler;
pub mod service;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::Utc;
use tokio::sync::{broadcast, Notify, RwLock};
use tracing;
use uuid::Uuid;

use crate::daemon::events::JobEvent;
use crate::daemon::executor::{Executor, RunHandle};
use crate::daemon::scheduler::Scheduler;
use crate::models::{DaemonConfig, RunStatus};
use crate::server::{self, AppState};
use crate::storage::LogStore;

// ---------------------------------------------------------------------------
// PidFile — exclusive PID file acquisition
// ---------------------------------------------------------------------------

/// Manages a PID file to ensure only one daemon instance runs at a time.
///
/// Uses exclusive file creation (CREATE_NEW / O_EXCL) to prevent races.
/// If the PID file exists, checks whether the recorded PID is still alive.
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// Create a new PidFile handle (does not acquire yet).
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Acquire the PID file.
    ///
    /// - If the file does not exist, creates it exclusively and writes the
    ///   current PID.
    /// - If the file exists and the recorded PID is alive, returns an error.
    /// - If the file exists but the PID is stale (process dead), removes the
    ///   stale file and acquires.
    pub fn acquire(&self) -> Result<()> {
        if self.path.exists() {
            // Read existing PID
            let content =
                std::fs::read_to_string(&self.path).context("Failed to read existing PID file")?;
            let existing_pid: u32 = content
                .trim()
                .parse()
                .context("Failed to parse PID from PID file")?;

            if is_process_alive(existing_pid) {
                return Err(anyhow::anyhow!(
                    "Daemon is already running (PID {existing_pid}). \
                     PID file: {}",
                    self.path.display()
                ));
            }

            // Stale PID file — remove it
            tracing::warn!(
                "Removing stale PID file (PID {} is no longer running)",
                existing_pid
            );
            std::fs::remove_file(&self.path).context("Failed to remove stale PID file")?;
        }

        // Create the file exclusively (CREATE_NEW / O_EXCL)
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.path)
            .context("Failed to create PID file (exclusive create)")?;

        let pid = std::process::id();
        write!(file, "{}", pid).context("Failed to write PID to PID file")?;
        file.flush().context("Failed to flush PID file")?;

        tracing::info!("PID file acquired: {} (PID {})", self.path.display(), pid);
        Ok(())
    }

    /// Release the PID file by removing it.
    pub fn release(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path).context("Failed to remove PID file")?;
            tracing::info!("PID file released: {}", self.path.display());
        }
        Ok(())
    }

    /// Check if the PID file exists and the recorded process is alive.
    pub fn is_alive(&self) -> bool {
        if !self.path.exists() {
            return false;
        }
        match std::fs::read_to_string(&self.path) {
            Ok(content) => match content.trim().parse::<u32>() {
                Ok(pid) => is_process_alive(pid),
                Err(_) => false,
            },
            Err(_) => false,
        }
    }

    /// Return the path to this PID file.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Check whether a process with the given PID is alive.
///
/// - Unix: uses kill(pid, 0) — signal 0 checks existence without sending a
///   signal.
/// - Windows: uses OpenProcess with PROCESS_QUERY_LIMITED_INFORMATION.
pub fn is_process_alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        // signal 0 tests process existence
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }

    #[cfg(windows)]
    {
        // PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                false
            } else {
                CloseHandle(handle);
                true
            }
        }
    }
}

#[cfg(windows)]
extern "system" {
    fn OpenProcess(
        dwDesiredAccess: u32,
        bInheritHandle: i32,
        dwProcessId: u32,
    ) -> *mut std::ffi::c_void;
    fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
}

// ---------------------------------------------------------------------------
// Config loading
// ---------------------------------------------------------------------------

/// Load the DaemonConfig using the resolution order from the SPEC:
///   1. --config CLI flag (passed as config_path)
///   2. ACS_CONFIG_DIR environment variable
///   3. Platform config dir (dirs::config_dir()/agent-cron-scheduler/config.json)
///   4. Fall back to {data_dir}/config.json
///   5. If no config file exists, use DaemonConfig::default()
pub fn load_config(config_path: Option<&Path>) -> Result<DaemonConfig> {
    // 1. Explicit config path
    if let Some(path) = config_path {
        if path.exists() {
            let content = std::fs::read_to_string(path).context("Failed to read config file")?;
            let config: DaemonConfig =
                serde_json::from_str(&content).context("Failed to parse config file")?;
            tracing::info!("Loaded config from: {}", path.display());
            return Ok(config);
        }
        return Err(anyhow::anyhow!("Config file not found: {}", path.display()));
    }

    // 2. ACS_CONFIG_DIR env var
    if let Ok(config_dir) = std::env::var("ACS_CONFIG_DIR") {
        let path = PathBuf::from(&config_dir).join("config.json");
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .context("Failed to read config from ACS_CONFIG_DIR")?;
            let config: DaemonConfig = serde_json::from_str(&content)
                .context("Failed to parse config from ACS_CONFIG_DIR")?;
            tracing::info!("Loaded config from ACS_CONFIG_DIR: {}", path.display());
            return Ok(config);
        }
    }

    // 3. Platform config dir
    if let Some(config_dir) = dirs::config_dir() {
        let path = config_dir.join("agent-cron-scheduler").join("config.json");
        if path.exists() {
            let content = std::fs::read_to_string(&path)
                .context("Failed to read config from platform config dir")?;
            let config: DaemonConfig = serde_json::from_str(&content)
                .context("Failed to parse config from platform config dir")?;
            tracing::info!("Loaded config from: {}", path.display());
            return Ok(config);
        }
    }

    // 4. Fall back to data_dir/config.json
    let data_dir = resolve_data_dir(None);
    let path = data_dir.join("config.json");
    if path.exists() {
        let content =
            std::fs::read_to_string(&path).context("Failed to read config from data dir")?;
        let config: DaemonConfig =
            serde_json::from_str(&content).context("Failed to parse config from data dir")?;
        tracing::info!("Loaded config from: {}", path.display());
        return Ok(config);
    }

    // 5. Use defaults
    tracing::info!("No config file found, using defaults");
    Ok(DaemonConfig::default())
}

/// Resolve the data directory. If `override_dir` is Some, use it.
/// Otherwise, use the platform default.
///
/// Platform defaults:
/// - Windows: `C:\ProgramData\agent-cron-scheduler` (shared across all users,
///   appropriate for services running under LOCAL SYSTEM)
/// - macOS/Linux: `~/.local/share/agent-cron-scheduler` via `dirs::data_dir()`
pub fn resolve_data_dir(override_dir: Option<&Path>) -> PathBuf {
    if let Some(dir) = override_dir {
        return dir.to_path_buf();
    }

    // Check ACS_DATA_DIR env
    if let Ok(d) = std::env::var("ACS_DATA_DIR") {
        return PathBuf::from(d);
    }

    // Platform default
    #[cfg(target_os = "windows")]
    {
        // Use ProgramData on Windows - appropriate for services and shared across users
        std::env::var("PROGRAMDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("C:\\ProgramData"))
            .join("agent-cron-scheduler")
    }

    #[cfg(not(target_os = "windows"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("agent-cron-scheduler")
    }
}

/// Remove log directories for jobs that no longer exist in the job store.
///
/// Scans the `logs/` directory for subdirectories named after job UUIDs,
/// compares against the known job IDs, and removes orphaned directories.
pub async fn cleanup_orphaned_logs(
    data_dir: &Path,
    job_store: &dyn crate::storage::JobStore,
) -> Result<()> {
    let logs_dir = data_dir.join("logs");
    if !logs_dir.exists() {
        return Ok(());
    }

    // Get known job IDs
    let jobs = job_store
        .list_jobs()
        .await
        .context("Failed to list jobs for orphan cleanup")?;
    let known_ids: std::collections::HashSet<String> =
        jobs.iter().map(|j| j.id.to_string()).collect();

    // Scan the logs directory
    let mut entries = tokio::fs::read_dir(&logs_dir)
        .await
        .context("Failed to read logs directory")?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
            // Check if this is a valid UUID and if the job still exists
            if Uuid::parse_str(dir_name).is_ok() && !known_ids.contains(dir_name) {
                tracing::info!("Removing orphaned log directory: {}", dir_name);
                if let Err(e) = tokio::fs::remove_dir_all(&path).await {
                    tracing::warn!(
                        "Failed to remove orphaned log directory {}: {}",
                        dir_name,
                        e
                    );
                }
            }
        }
    }

    Ok(())
}

/// Create the required data directories under `data_dir`.
pub async fn create_data_dirs(data_dir: &Path) -> Result<()> {
    tokio::fs::create_dir_all(data_dir)
        .await
        .context("Failed to create data directory")?;
    tokio::fs::create_dir_all(data_dir.join("logs"))
        .await
        .context("Failed to create logs directory")?;
    tokio::fs::create_dir_all(data_dir.join("scripts"))
        .await
        .context("Failed to create scripts directory")?;
    tracing::info!("Data directories ensured at: {}", data_dir.display());
    Ok(())
}

// ---------------------------------------------------------------------------
// Graceful shutdown
// ---------------------------------------------------------------------------

/// Perform the graceful shutdown sequence per SPEC Section 8:
///
/// 1. Stop accepting new HTTP connections       (handled by caller dropping server)
/// 2. Stop scheduling new job runs              (handled by caller aborting scheduler)
/// 3. Kill all running child processes (30s grace)
/// 4. Update all in-flight JobRun records to Killed status
/// 5. Flush all log files                       (implicit with LogStore)
/// 6. Remove PID file
/// 7. Exit with code 0                          (handled by caller)
pub async fn graceful_shutdown(
    active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>>,
    log_store: Arc<dyn LogStore>,
    pid_file: Option<&PidFile>,
) {
    tracing::info!("Beginning graceful shutdown sequence...");

    // Step 3: Kill all running processes with 30s grace period
    let run_entries: Vec<(Uuid, Uuid)>;
    {
        let mut runs = active_runs.write().await;
        run_entries = runs
            .values()
            .map(|handle| (handle.job_id, handle.run_id))
            .collect();

        // Send kill signals to all active runs
        let keys: Vec<Uuid> = runs.keys().cloned().collect();
        for key in keys {
            if let Some(handle) = runs.remove(&key) {
                let _ = handle.kill_tx.send(());
                // Wait up to 30s for the task to finish
                let join_handle = handle.join_handle;
                let timeout_result =
                    tokio::time::timeout(std::time::Duration::from_secs(30), join_handle).await;

                match timeout_result {
                    Ok(Ok(())) => {
                        tracing::info!("Run {} shut down gracefully", key);
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Run {} task failed during shutdown: {}", key, e);
                    }
                    Err(_) => {
                        tracing::warn!("Run {} did not finish within 30s grace period", key);
                    }
                }
            }
        }
    }

    // Step 4: Update all in-flight JobRun records to Killed status
    for (job_id, run_id) in &run_entries {
        let (runs_list, _) = match log_store.list_runs(*job_id, 1000, 0).await {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("Failed to list runs for job {}: {}", job_id, e);
                continue;
            }
        };

        for run in runs_list {
            if run.run_id == *run_id && run.status == RunStatus::Running {
                let killed_run = crate::models::JobRun {
                    run_id: run.run_id,
                    job_id: run.job_id,
                    started_at: run.started_at,
                    finished_at: Some(Utc::now()),
                    status: RunStatus::Killed,
                    exit_code: None,
                    log_size_bytes: run.log_size_bytes,
                    error: Some("Daemon shutting down".to_string()),
                };
                if let Err(e) = log_store.update_run(&killed_run).await {
                    tracing::error!("Failed to mark run {} as Killed: {}", run_id, e);
                } else {
                    tracing::info!("Marked run {} as Killed", run_id);
                }
            }
        }
    }

    // Step 5: Flush all log files (implicit — LogStore writes are flushed)

    // Step 6: Remove PID file
    if let Some(pf) = pid_file {
        if let Err(e) = pf.release() {
            tracing::error!("Failed to release PID file: {}", e);
        }
    }

    tracing::info!("Graceful shutdown complete.");
}

// ---------------------------------------------------------------------------
// Daemon bootstrap
// ---------------------------------------------------------------------------

/// Start the daemon.
///
/// This is the main entry point for the background daemon process. It:
/// 1. Acquires PID file
/// 2. Loads config
/// 3. Creates data directories
/// 4. Initializes storage (JsonJobStore, FsLogStore)
/// 5. Creates broadcast channel
/// 6. Creates scheduler notify
/// 7. Starts Executor
/// 8. Starts Scheduler
/// 9. Starts HTTP server
/// 10. Sets up signal handling
/// 11. Runs shutdown sequence on signal
pub async fn start_daemon(
    config_path: Option<&Path>,
    data_dir_override: Option<&Path>,
    host_override: Option<&str>,
    port_override: Option<u16>,
    foreground: bool,
) -> Result<()> {
    // Load config
    let mut config = load_config(config_path)?;

    // Apply host/port overrides from CLI flags
    if let Some(h) = host_override {
        config.host = h.to_string();
    }
    if let Some(p) = port_override {
        config.port = p;
    }

    // Resolve data dir
    let data_dir = if let Some(d) = data_dir_override {
        d.to_path_buf()
    } else if let Some(ref d) = config.data_dir {
        d.clone()
    } else {
        resolve_data_dir(None)
    };
    config.data_dir = Some(data_dir.clone());

    let config = Arc::new(config);

    // Create data directories
    create_data_dirs(&data_dir).await?;

    // Acquire PID file
    let pid_file_path = data_dir.join("acs.pid");
    let pid_file = PidFile::new(pid_file_path);
    pid_file.acquire()?;

    // Initialize storage
    let job_store = Arc::new(crate::storage::jobs::JsonJobStore::new(data_dir.clone()).await?)
        as Arc<dyn crate::storage::JobStore>;

    let log_store = Arc::new(crate::storage::logs::FsLogStore::new(data_dir.clone()).await?)
        as Arc<dyn crate::storage::LogStore>;

    // Clean up orphaned log directories
    if let Err(e) = cleanup_orphaned_logs(&data_dir, job_store.as_ref()).await {
        tracing::warn!("Failed to cleanup orphaned logs: {}", e);
    }

    // Create broadcast channel
    let (event_tx, _event_rx) = broadcast::channel::<JobEvent>(config.broadcast_capacity);

    // Create scheduler notify
    let scheduler_notify = Arc::new(Notify::new());

    // Shutdown channel
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(());

    // Active runs tracking
    let active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>> = Arc::new(RwLock::new(HashMap::new()));

    // Create dispatch channel (used by both scheduler and API trigger)
    let (dispatch_tx, mut dispatch_rx) = tokio::sync::mpsc::channel::<crate::models::Job>(64);
    let dispatch_tx_for_api = dispatch_tx.clone();

    // Create AppState
    let state = Arc::new(AppState {
        job_store: Arc::clone(&job_store),
        log_store: Arc::clone(&log_store),
        event_tx: event_tx.clone(),
        scheduler_notify: Arc::clone(&scheduler_notify),
        config: Arc::clone(&config),
        start_time: Instant::now(),
        active_runs: Arc::clone(&active_runs),
        shutdown_tx: Some(shutdown_tx.clone()),
        dispatch_tx: Some(dispatch_tx_for_api),
    });

    // Create Executor
    // NoPtySpawner uses plain std::process::Command with piped I/O for process spawning.
    // This reliably handles EOF on all platforms.
    let pty_spawner: Arc<dyn crate::pty::PtySpawner> = Arc::new(crate::pty::NoPtySpawner);
    let executor = Executor::new(
        event_tx.clone(),
        Arc::clone(&log_store),
        Arc::clone(&config),
        pty_spawner,
    );

    // Start Scheduler
    let sched_clock: Arc<dyn scheduler::Clock> = Arc::new(scheduler::SystemClock);
    let scheduler = Scheduler::new(
        Arc::clone(&job_store),
        sched_clock,
        Arc::clone(&scheduler_notify),
        dispatch_tx,
    );

    let scheduler_handle = tokio::spawn(async move {
        if let Err(e) = scheduler.run().await {
            tracing::error!("Scheduler error: {}", e);
        }
    });

    // Dispatch loop: receives jobs from scheduler and spawns them via executor
    let dispatch_active_runs = Arc::clone(&active_runs);
    let dispatch_handle = tokio::spawn(async move {
        while let Some(job) = dispatch_rx.recv().await {
            match executor.spawn_job(&job).await {
                Ok(handle) => {
                    let job_id = handle.job_id;
                    dispatch_active_runs.write().await.insert(job_id, handle);
                }
                Err(e) => {
                    tracing::error!("Failed to spawn job {}: {}", job.name, e);
                }
            }
        }
    });

    // Job metadata updater: listens for job events and updates job store metadata,
    // and emits tracing log lines for job lifecycle events.
    let updater_job_store = Arc::clone(&job_store);
    let mut updater_rx = event_tx.subscribe();
    let updater_handle = tokio::spawn(async move {
        loop {
            match updater_rx.recv().await {
                Ok(JobEvent::Started {
                    job_name, run_id, ..
                }) => {
                    tracing::info!("Job '{}' started (run: {})", job_name, run_id);
                }
                Ok(JobEvent::Completed {
                    job_id,
                    run_id,
                    exit_code,
                    timestamp,
                }) => {
                    tracing::info!("Job run {} completed (exit code: {})", run_id, exit_code);
                    let update = crate::models::JobUpdate {
                        last_run_at: Some(Some(timestamp)),
                        last_exit_code: Some(Some(exit_code)),
                        ..Default::default()
                    };
                    if let Err(e) = updater_job_store.update_job(job_id, update).await {
                        tracing::error!("Failed to update job metadata after completion: {}", e);
                    }
                }
                Ok(JobEvent::Failed {
                    job_id,
                    run_id,
                    ref error,
                    timestamp,
                }) => {
                    tracing::warn!("Job run {} failed: {}", run_id, error);
                    let update = crate::models::JobUpdate {
                        last_run_at: Some(Some(timestamp)),
                        ..Default::default()
                    };
                    if let Err(e) = updater_job_store.update_job(job_id, update).await {
                        tracing::error!("Failed to update job metadata after failure: {}", e);
                    }
                }
                Ok(_) => {} // Ignore other events (Output, JobChanged)
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!("Job metadata updater lagged by {} events", n);
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
            }
        }
    });

    // Create router and start HTTP server
    let router = server::create_router(Arc::clone(&state));
    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context(format!("Failed to bind to {}", bind_addr))?;

    tracing::info!("Daemon started. Listening on http://{}", bind_addr);

    if foreground {
        tracing::info!("Running in foreground mode. Press Ctrl+C to stop.");
    }

    // Start server with graceful shutdown support
    let server_handle = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                shutdown_rx.changed().await.ok();
                tracing::info!("HTTP server received shutdown signal");
            })
            .await
            .ok();
    });

    // Wait for shutdown: Ctrl+C, SIGTERM (Unix), or API shutdown request.
    // The API shutdown subscriber ensures `acs stop` actually terminates the process
    // even when running headless (no console to send Ctrl+C).
    let mut api_shutdown_rx = shutdown_tx.subscribe();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C signal");
            }
            _ = sigterm.recv() => {
                tracing::info!("Received SIGTERM signal");
            }
            _ = api_shutdown_rx.changed() => {
                tracing::info!("Received API shutdown signal");
            }
        }
    }
    #[cfg(not(unix))]
    {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("Received Ctrl+C signal");
            }
            _ = api_shutdown_rx.changed() => {
                tracing::info!("Received API shutdown signal");
            }
        }
    }

    // Send shutdown signal to HTTP server
    let _ = shutdown_tx.send(());

    // Stop scheduler, dispatch loop, and updater
    scheduler_handle.abort();
    dispatch_handle.abort();
    updater_handle.abort();

    // Run graceful shutdown sequence
    graceful_shutdown(
        Arc::clone(&active_runs),
        Arc::clone(&log_store),
        Some(&pid_file),
    )
    .await;

    // Wait for HTTP server to finish
    let _ = server_handle.await;

    tracing::info!("Daemon exited cleanly.");
    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::executor::RunHandle;
    use crate::models::{JobRun, RunStatus};
    use crate::storage::LogStore;
    use async_trait::async_trait;
    use tempfile::TempDir;
    use tokio::sync::RwLock;

    // -----------------------------------------------------------------------
    // InMemoryLogStore for shutdown tests
    // -----------------------------------------------------------------------

    struct InMemoryLogStore {
        runs: RwLock<Vec<JobRun>>,
        logs: RwLock<HashMap<(Uuid, Uuid), Vec<u8>>>,
    }

    impl InMemoryLogStore {
        fn new() -> Self {
            Self {
                runs: RwLock::new(Vec::new()),
                logs: RwLock::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl LogStore for InMemoryLogStore {
        async fn create_run(&self, run: &JobRun) -> anyhow::Result<()> {
            self.runs.write().await.push(run.clone());
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
            let entry = logs.entry((job_id, run_id)).or_default();
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

        async fn cleanup(&self, _job_id: Uuid, _max_files: usize) -> anyhow::Result<()> {
            Ok(())
        }
    }

    // =======================================================================
    // 1. PidFile acquire creates file (exclusive create)
    // =======================================================================
    #[test]
    fn test_pidfile_acquire_creates_file() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire should succeed");

        // Verify the PID file was created
        assert!(pid_path.exists(), "PID file should exist after acquire");

        // Verify it contains the current PID
        let content = std::fs::read_to_string(&pid_path).expect("read PID file");
        let written_pid: u32 = content.trim().parse().expect("parse PID");
        assert_eq!(
            written_pid,
            std::process::id(),
            "PID file should contain the current process PID"
        );

        // Cleanup
        pid_file.release().expect("release");
    }

    // =======================================================================
    // 2. PidFile acquire fails if already held by live process
    // =======================================================================
    #[test]
    fn test_pidfile_acquire_fails_if_held_by_live_process() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        // Write a PID file with the current process's PID (which is alive)
        std::fs::write(&pid_path, format!("{}", std::process::id())).expect("write PID file");

        let pid_file = PidFile::new(pid_path.clone());
        let result = pid_file.acquire();

        assert!(
            result.is_err(),
            "Acquire should fail when PID file is held by a live process"
        );

        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("already running"),
            "Error should mention 'already running', got: {}",
            err_msg
        );
    }

    // =======================================================================
    // 3. PidFile acquire succeeds if PID file is stale (dead process)
    // =======================================================================
    #[test]
    fn test_pidfile_acquire_succeeds_if_stale() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        // Write a PID that is extremely unlikely to be alive.
        // PID 99999999 should not exist on any normal system.
        // On Windows, the max PID is around 4 million.
        let stale_pid: u32 = 4_000_000;
        std::fs::write(&pid_path, format!("{}", stale_pid)).expect("write stale PID file");

        let pid_file = PidFile::new(pid_path.clone());
        let result = pid_file.acquire();

        assert!(
            result.is_ok(),
            "Acquire should succeed when PID file is stale: {:?}",
            result.err()
        );

        // Verify it now contains our PID
        let content = std::fs::read_to_string(&pid_path).expect("read PID file");
        let written_pid: u32 = content.trim().parse().expect("parse PID");
        assert_eq!(
            written_pid,
            std::process::id(),
            "PID file should now contain the current process PID"
        );

        // Cleanup
        pid_file.release().expect("release");
    }

    // =======================================================================
    // 4. PidFile release removes file
    // =======================================================================
    #[test]
    fn test_pidfile_release_removes_file() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire");

        assert!(pid_path.exists(), "PID file should exist before release");

        pid_file.release().expect("release");

        assert!(
            !pid_path.exists(),
            "PID file should NOT exist after release"
        );
    }

    // =======================================================================
    // 5. Shutdown sequence marks running jobs as Killed
    // =======================================================================
    #[tokio::test]
    async fn test_shutdown_marks_running_jobs_as_killed() {
        let log_store = Arc::new(InMemoryLogStore::new());

        let job_id = Uuid::now_v7();
        let run_id = Uuid::now_v7();

        // Create a Running job run in the log store
        let running_run = JobRun {
            run_id,
            job_id,
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            exit_code: None,
            log_size_bytes: 0,
            error: None,
        };
        log_store.create_run(&running_run).await.unwrap();

        // Create a fake active run handle
        let (kill_tx, _kill_rx) = tokio::sync::oneshot::channel::<()>();
        let join_handle = tokio::spawn(async {
            // Simulate a long-running task that finishes quickly on shutdown
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        });

        let active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>> =
            Arc::new(RwLock::new(HashMap::new()));
        active_runs.write().await.insert(
            job_id,
            RunHandle {
                run_id,
                job_id,
                join_handle,
                kill_tx,
            },
        );

        // Run graceful shutdown (no PID file for this test)
        graceful_shutdown(
            Arc::clone(&active_runs),
            Arc::clone(&log_store) as Arc<dyn LogStore>,
            None,
        )
        .await;

        // Verify the run was marked as Killed
        let runs = log_store.runs.read().await;
        let run = runs
            .iter()
            .find(|r| r.run_id == run_id)
            .expect("run exists");
        assert_eq!(
            run.status,
            RunStatus::Killed,
            "Running job should be marked as Killed during shutdown"
        );
        assert!(
            run.finished_at.is_some(),
            "Killed run should have a finished_at timestamp"
        );
        assert!(
            run.error.is_some(),
            "Killed run should have an error message"
        );
        assert!(
            run.error.as_ref().unwrap().contains("shutting down"),
            "Error should mention shutdown"
        );
    }

    // =======================================================================
    // 6. Service detection (is_service_registered)
    // =======================================================================
    #[test]
    fn test_service_detection() {
        // We use the service module's is_service_registered function.
        // On dev machines / CI, the service is typically NOT registered.
        // This test verifies the function runs without panic.
        let registered = service::is_service_registered();
        // On a typical test environment, the service should NOT be registered
        // but we cannot guarantee that, so we just ensure it returns a bool.
        let _: bool = registered;

        // Also verify service_status returns valid data
        let status = service::service_status();
        assert!(
            status.platform == "windows"
                || status.platform == "macos"
                || status.platform == "linux",
            "Platform should be a known OS"
        );
    }

    // =======================================================================
    // 7. Config loading with defaults
    // =======================================================================
    #[test]
    fn test_config_loading_returns_defaults_when_no_file() {
        let config = load_config(None).expect("load config");
        assert_eq!(config.host, "127.0.0.1");
        assert_eq!(config.port, 8377);
        assert_eq!(config.broadcast_capacity, 4096);
        assert_eq!(config.max_log_files_per_job, 50);
        assert_eq!(config.default_timeout_secs, 0);
        assert_eq!(config.pty_rows, 24);
        assert_eq!(config.pty_cols, 80);
    }

    #[test]
    fn test_config_loading_from_file() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let config_path = tmp_dir.path().join("config.json");
        std::fs::write(&config_path, r#"{"port": 9999, "host": "0.0.0.0"}"#).expect("write config");

        let config = load_config(Some(&config_path)).expect("load config");
        assert_eq!(config.host, "0.0.0.0");
        assert_eq!(config.port, 9999);
        // Other fields should be defaults
        assert_eq!(config.broadcast_capacity, 4096);
    }

    #[test]
    fn test_config_loading_nonexistent_explicit_path_fails() {
        let result = load_config(Some(Path::new("/nonexistent/config.json")));
        assert!(result.is_err(), "Should fail for nonexistent explicit path");
    }

    // =======================================================================
    // 8. Data directory creation
    // =======================================================================
    #[tokio::test]
    async fn test_data_directory_creation() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let data_dir = tmp_dir.path().join("acs-data");

        assert!(
            !data_dir.exists(),
            "Data dir should not exist before creation"
        );

        create_data_dirs(&data_dir).await.expect("create dirs");

        assert!(data_dir.exists(), "Data dir should exist");
        assert!(
            data_dir.join("logs").exists(),
            "logs subdirectory should exist"
        );
        assert!(
            data_dir.join("scripts").exists(),
            "scripts subdirectory should exist"
        );
    }

    #[tokio::test]
    async fn test_data_directory_creation_idempotent() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let data_dir = tmp_dir.path().join("acs-data");

        // Create twice — should not fail
        create_data_dirs(&data_dir).await.expect("first create");
        create_data_dirs(&data_dir).await.expect("second create");

        assert!(data_dir.exists());
        assert!(data_dir.join("logs").exists());
        assert!(data_dir.join("scripts").exists());
    }

    // =======================================================================
    // Additional PID file tests
    // =======================================================================

    #[test]
    fn test_pidfile_is_alive_true_when_acquired() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire");

        assert!(
            pid_file.is_alive(),
            "is_alive should return true when PID file exists with our PID"
        );

        pid_file.release().expect("release");
    }

    #[test]
    fn test_pidfile_is_alive_false_when_released() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire");
        pid_file.release().expect("release");

        assert!(
            !pid_file.is_alive(),
            "is_alive should return false after release"
        );
    }

    #[test]
    fn test_pidfile_is_alive_false_when_not_created() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("nonexistent.pid");

        let pid_file = PidFile::new(pid_path);
        assert!(
            !pid_file.is_alive(),
            "is_alive should return false when PID file does not exist"
        );
    }

    #[test]
    fn test_pidfile_release_is_idempotent() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire");

        // Release twice — second should not error
        pid_file.release().expect("first release");
        pid_file
            .release()
            .expect("second release should also succeed");
    }

    #[test]
    fn test_is_process_alive_for_current_process() {
        let pid = std::process::id();
        assert!(is_process_alive(pid), "Current process PID should be alive");
    }

    #[test]
    fn test_is_process_alive_for_dead_process() {
        // Use a very high PID that is unlikely to exist
        let dead_pid: u32 = 4_000_000;
        assert!(
            !is_process_alive(dead_pid),
            "PID 4000000 should not be alive"
        );
    }

    #[test]
    fn test_resolve_data_dir_with_override() {
        let path = PathBuf::from("/custom/data");
        let resolved = resolve_data_dir(Some(&path));
        assert_eq!(resolved, path);
    }

    #[test]
    fn test_resolve_data_dir_default_not_empty() {
        let resolved = resolve_data_dir(None);
        assert!(
            !resolved.to_string_lossy().is_empty(),
            "Default data dir should not be empty"
        );
        // On all platforms, the path should end with agent-cron-scheduler
        // (unless ACS_DATA_DIR is set in the environment)
        if std::env::var("ACS_DATA_DIR").is_err() {
            assert!(
                resolved.to_string_lossy().contains("agent-cron-scheduler"),
                "Default data dir should contain 'agent-cron-scheduler', got: {}",
                resolved.display()
            );
        }
    }

    // =======================================================================
    // Shutdown with PID file release
    // =======================================================================
    #[tokio::test]
    async fn test_shutdown_releases_pid_file() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire");

        assert!(pid_path.exists(), "PID file should exist before shutdown");

        let active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let log_store = Arc::new(InMemoryLogStore::new()) as Arc<dyn LogStore>;

        graceful_shutdown(active_runs, log_store, Some(&pid_file)).await;

        assert!(
            !pid_path.exists(),
            "PID file should be removed after shutdown"
        );
    }

    // =======================================================================
    // Shutdown with no active runs (empty case)
    // =======================================================================
    #[tokio::test]
    async fn test_shutdown_with_no_active_runs() {
        let active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let log_store = Arc::new(InMemoryLogStore::new()) as Arc<dyn LogStore>;

        // Should complete without error
        graceful_shutdown(active_runs, log_store, None).await;
    }

    // =======================================================================
    // Orphaned log cleanup tests
    // =======================================================================

    struct InMemoryJobStore {
        jobs: RwLock<Vec<crate::models::Job>>,
    }

    impl InMemoryJobStore {
        fn new() -> Self {
            Self {
                jobs: RwLock::new(Vec::new()),
            }
        }

        async fn add_job(&self, job: crate::models::Job) {
            self.jobs.write().await.push(job);
        }
    }

    #[async_trait]
    impl crate::storage::JobStore for InMemoryJobStore {
        async fn list_jobs(&self) -> anyhow::Result<Vec<crate::models::Job>> {
            Ok(self.jobs.read().await.clone())
        }
        async fn get_job(&self, id: Uuid) -> anyhow::Result<Option<crate::models::Job>> {
            Ok(self.jobs.read().await.iter().find(|j| j.id == id).cloned())
        }
        async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<crate::models::Job>> {
            Ok(self
                .jobs
                .read()
                .await
                .iter()
                .find(|j| j.name == name)
                .cloned())
        }
        async fn create_job(
            &self,
            _new: crate::models::NewJob,
        ) -> anyhow::Result<crate::models::Job> {
            unimplemented!()
        }
        async fn update_job(
            &self,
            _id: Uuid,
            _update: crate::models::JobUpdate,
        ) -> anyhow::Result<crate::models::Job> {
            unimplemented!()
        }
        async fn delete_job(&self, _id: Uuid) -> anyhow::Result<()> {
            unimplemented!()
        }
    }

    fn make_test_job(id: Uuid) -> crate::models::Job {
        let now = Utc::now();
        crate::models::Job {
            id,
            name: format!("test-{}", id),
            schedule: "*/5 * * * *".to_string(),
            execution: crate::models::ExecutionType::ShellCommand("echo hi".to_string()),
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

    #[tokio::test]
    async fn test_cleanup_orphaned_logs_removes_unknown_dirs() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let data_dir = tmp_dir.path().to_path_buf();
        let logs_dir = data_dir.join("logs");
        tokio::fs::create_dir_all(&logs_dir)
            .await
            .expect("create logs dir");

        let known_id = Uuid::now_v7();
        let orphan_id = Uuid::now_v7();

        // Create log directories for both known and orphaned jobs
        tokio::fs::create_dir_all(logs_dir.join(known_id.to_string()))
            .await
            .expect("create known log dir");
        tokio::fs::create_dir_all(logs_dir.join(orphan_id.to_string()))
            .await
            .expect("create orphan log dir");

        // Write a file in the orphan dir to ensure it gets fully removed
        tokio::fs::write(
            logs_dir.join(orphan_id.to_string()).join("test.log"),
            b"orphaned log data",
        )
        .await
        .expect("write orphan log");

        // Create a job store with only the known job
        let job_store = InMemoryJobStore::new();
        job_store.add_job(make_test_job(known_id)).await;

        // Run cleanup
        cleanup_orphaned_logs(&data_dir, &job_store)
            .await
            .expect("cleanup should succeed");

        // Known job's log dir should still exist
        assert!(
            logs_dir.join(known_id.to_string()).exists(),
            "Known job's log directory should be preserved"
        );

        // Orphaned job's log dir should be removed
        assert!(
            !logs_dir.join(orphan_id.to_string()).exists(),
            "Orphaned log directory should be removed"
        );
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_logs_preserves_non_uuid_dirs() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let data_dir = tmp_dir.path().to_path_buf();
        let logs_dir = data_dir.join("logs");
        tokio::fs::create_dir_all(&logs_dir)
            .await
            .expect("create logs dir");

        // Create a non-UUID directory
        tokio::fs::create_dir_all(logs_dir.join("not-a-uuid"))
            .await
            .expect("create non-uuid dir");

        let job_store = InMemoryJobStore::new();
        cleanup_orphaned_logs(&data_dir, &job_store)
            .await
            .expect("cleanup should succeed");

        // Non-UUID directory should be preserved
        assert!(
            logs_dir.join("not-a-uuid").exists(),
            "Non-UUID directories should not be touched"
        );
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_logs_no_logs_dir() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let data_dir = tmp_dir.path().to_path_buf();

        // No logs dir created -- should succeed silently
        let job_store = InMemoryJobStore::new();
        let result = cleanup_orphaned_logs(&data_dir, &job_store).await;
        assert!(result.is_ok(), "Should succeed when logs dir doesn't exist");
    }

    #[tokio::test]
    async fn test_cleanup_orphaned_logs_empty_logs_dir() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let data_dir = tmp_dir.path().to_path_buf();
        let logs_dir = data_dir.join("logs");
        tokio::fs::create_dir_all(&logs_dir)
            .await
            .expect("create logs dir");

        let job_store = InMemoryJobStore::new();
        let result = cleanup_orphaned_logs(&data_dir, &job_store).await;
        assert!(result.is_ok(), "Should succeed with empty logs dir");
    }
}
