// Daemon module - Phase 6+ implementation (workflow-native runtime)
// Sub-modules for events, scheduler, and service.

pub mod events;
pub mod scheduler;
pub mod service;

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use tokio::sync::{broadcast, Notify, RwLock};
use tracing;
use uuid::Uuid;

use crate::models::DaemonConfig;
use crate::server::{self, AppState};
use crate::daemon::events::WorkflowEvent;

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
                // The existing process is alive — it may be shutting down
                // (e.g., during a restart). Retry for up to 10 seconds.
                let mut acquired = false;
                for attempt in 0..20 {
                    tracing::info!(
                        "PID {} is still alive, waiting for it to exit (attempt {}/20)...",
                        existing_pid,
                        attempt + 1
                    );
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    if !is_process_alive(existing_pid) {
                        acquired = true;
                        break;
                    }
                }
                if !acquired {
                    return Err(anyhow::anyhow!(
                        "Daemon is already running (PID {existing_pid}). \
                         PID file: {}",
                        self.path.display()
                    ));
                }
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
/// - Windows: uses OpenProcess + GetExitCodeProcess. OpenProcess alone is not
///   sufficient because it can succeed on a dead process if another process
///   (e.g., the Electron parent or Windows Task Scheduler) still holds a
///   handle to it, keeping the kernel object alive. We additionally check
///   GetExitCodeProcess — if the exit code is not STILL_ACTIVE (259), the
///   process is dead despite the handle being valid.
///
///   NOTE: We observed this zombie-handle scenario once in production (PID
///   33772 reported alive across 20 retries while `taskkill` said "not found").
///   A reboot cleared the state and we were unable to reproduce it by
///   repeating the same steps (launch via Electron, task-kill acs.exe, close
///   Electron, re-run acs start). This fix is applied defensively.
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
        // STILL_ACTIVE is the exit code for a process that hasn't exited yet.
        const STILL_ACTIVE: u32 = 259;

        unsafe {
            let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if handle.is_null() {
                return false;
            }

            // Defensive check: even if OpenProcess succeeds, verify the
            // process hasn't already exited (zombie handle scenario).
            let mut exit_code: u32 = 0;
            let result = GetExitCodeProcess(handle, &mut exit_code);
            CloseHandle(handle);

            // If GetExitCodeProcess fails, assume alive (conservative).
            // If it succeeds, the process is only alive if exit_code == STILL_ACTIVE.
            result != 0 && exit_code == STILL_ACTIVE
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
    fn GetExitCodeProcess(hProcess: *mut std::ffi::c_void, lpExitCode: *mut u32) -> i32;
    fn CloseHandle(hObject: *mut std::ffi::c_void) -> i32;
}

// ---------------------------------------------------------------------------
// PortFile — writes the server's bound port to a discoverable file
// ---------------------------------------------------------------------------

/// Manages a port file so the frontend (and CLI) can discover which port the
/// daemon is listening on. The file is written after the server binds and
/// removed during graceful shutdown.
pub struct PortFile {
    path: PathBuf,
}

impl PortFile {
    /// Create a new PortFile handle (does not write yet).
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    /// Write the given port number to the port file in the default data
    /// directory.
    pub fn write(port: u16) -> Result<Self> {
        let data_dir = resolve_data_dir(None);
        let path = data_dir.join("agentcronsystem.port");
        Self::write_to(path, port)
    }

    /// Write the given port number to a specific path (useful for tests and
    /// when the data directory is already known).
    pub fn write_to(path: PathBuf, port: u16) -> Result<Self> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .context("Failed to create port file")?;

        write!(file, "{}", port).context("Failed to write port to port file")?;
        file.flush().context("Failed to flush port file")?;

        tracing::info!("Port file written: {} (port {})", path.display(), port);
        Ok(Self { path })
    }

    /// Read the port number from the port file in the given data directory,
    /// returning `None` if the file does not exist or contains invalid data.
    pub fn read(data_dir: &Path) -> Option<u16> {
        let path = data_dir.join("agentcronsystem.port");
        Self::read_from(&path)
    }

    /// Read the port number from a specific path.
    pub fn read_from(path: &Path) -> Option<u16> {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|content| content.trim().parse::<u16>().ok())
    }

    /// Remove the port file. Succeeds silently if the file does not exist.
    pub fn remove(&self) -> Result<()> {
        if self.path.exists() {
            std::fs::remove_file(&self.path).context("Failed to remove port file")?;
            tracing::info!("Port file removed: {}", self.path.display());
        }
        Ok(())
    }

    /// Return the path to this port file.
    pub fn path(&self) -> &Path {
        &self.path
    }
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
/// - Windows: `%LOCALAPPDATA%\agent-cron-scheduler` (per-user, no admin required)
/// - macOS: `~/Library/Application Support/agent-cron-scheduler` via `dirs::data_dir()`
/// - Linux: `~/.local/share/agent-cron-scheduler` via `dirs::data_dir()`
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
        // Use LOCALAPPDATA on Windows — writable without admin elevation.
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .expect("LOCALAPPDATA environment variable must be set on Windows")
            .join("agent-cron-scheduler")
    }

    #[cfg(not(target_os = "windows"))]
    {
        dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("agent-cron-scheduler")
    }
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

/// Perform the graceful shutdown sequence:
///
/// 1. Stop accepting new HTTP connections       (handled by caller dropping server)
/// 2. Stop scheduling new workflow runs         (handled by caller aborting scheduler)
/// 3. Remove PID file and port file
/// 4. Exit with code 0                          (handled by caller)
pub async fn graceful_shutdown(
    pid_file: Option<&PidFile>,
    port_file: Option<&PortFile>,
) {
    tracing::info!("Beginning graceful shutdown sequence...");

    // Remove PID file and port file
    if let Some(pf) = pid_file {
        if let Err(e) = pf.release() {
            tracing::error!("Failed to release PID file: {}", e);
        }
    }
    if let Some(pf) = port_file {
        if let Err(e) = pf.remove() {
            tracing::error!("Failed to remove port file: {}", e);
        }
    }

    tracing::info!("Graceful shutdown complete.");
}

// ---------------------------------------------------------------------------
// SizeManagedWriter — daemon.log file writer with automatic size management
// ---------------------------------------------------------------------------

/// Maximum daemon.log file size before truncation (1 GB).
const DAEMON_LOG_MAX_BYTES: u64 = 1_073_741_824;

/// A file writer that tracks cumulative bytes written and automatically
/// truncates the oldest 25% of the file when it exceeds `max_size`.
///
/// This is used as the underlying writer for `tracing_appender::non_blocking`
/// so the daemon.log file never grows unbounded.
struct SizeManagedWriter {
    file: std::fs::File,
    path: PathBuf,
    bytes_written: u64,
    max_size: u64,
}

impl SizeManagedWriter {
    /// Create a new SizeManagedWriter.
    ///
    /// Opens the file at `path` in create+append mode and seeds `bytes_written`
    /// from the current file size so that truncation triggers correctly even
    /// if the file already has content.
    fn new(path: PathBuf, max_size: u64) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)?;
        let bytes_written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            file,
            path,
            bytes_written,
            max_size,
        })
    }

    /// Drop the oldest 25% of the file, keeping the newest 75%.
    ///
    /// Reads the entire file, finds the 25% byte offset, advances to the
    /// next newline boundary so we don't cut a line in half, then rewrites
    /// the file with only the retained portion.
    fn truncate_oldest_quarter(&mut self) -> std::io::Result<()> {
        let content = std::fs::read(&self.path)?;
        if content.is_empty() {
            self.bytes_written = 0;
            return Ok(());
        }

        let quarter = content.len() / 4;

        // Find the next newline after the 25% mark so we don't split a line.
        let cut_point = match content[quarter..].iter().position(|&b| b == b'\n') {
            Some(offset) => quarter + offset + 1, // skip past the newline
            None => {
                // No newline found after the 25% mark — keep everything
                // (degenerate case: single very long line).
                self.bytes_written = content.len() as u64;
                return Ok(());
            }
        };

        if cut_point >= content.len() {
            // Nothing left to keep after the cut — just truncate completely.
            self.file = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&self.path)?;
            // Reopen in append mode for subsequent writes.
            self.file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            self.bytes_written = 0;
            return Ok(());
        }

        let retained = &content[cut_point..];

        // Write retained content to a temporary file next to the log, then
        // replace the original. This avoids partial-write corruption if the
        // process is killed mid-write.
        let tmp_path = self.path.with_extension("log.tmp");
        std::fs::write(&tmp_path, retained)?;
        std::fs::rename(&tmp_path, &self.path)?;

        // Reopen the file in append mode.
        self.file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        self.bytes_written = retained.len() as u64;

        Ok(())
    }
}

impl Write for SizeManagedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.file.write(buf)?;
        self.bytes_written += n as u64;
        if self.bytes_written >= self.max_size {
            if let Err(e) = self.truncate_oldest_quarter() {
                // Log a warning but don't fail the write — losing some log
                // rotation is better than crashing the daemon's tracing pipeline.
                eprintln!(
                    "WARNING: daemon.log truncation failed: {}. Log file may grow beyond {}.",
                    e, self.max_size
                );
            }
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
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

    // Run migration: convert legacy jobs.json → workflows.json if needed
    match crate::migration::migrate_if_needed(&data_dir).await {
        Ok(crate::migration::MigrationResult::Migrated { count }) => {
            tracing::info!("Migrated {} legacy job(s) to workflows format", count);
        }
        Ok(crate::migration::MigrationResult::AlreadyMigrated) => {
            tracing::debug!("Migration skipped: workflows.json already exists");
        }
        Ok(crate::migration::MigrationResult::NotNeeded) => {
            tracing::debug!("Migration not needed: no legacy jobs.json found");
        }
        Err(e) => {
            tracing::error!("Migration failed: {}", e);
            // Non-fatal: continue startup even if migration fails
        }
    }

    // Set up tracing: always stderr, optionally also daemon.log file
    {
        use tracing_subscriber::layer::SubscriberExt;
        use tracing_subscriber::util::SubscriberInitExt;

        let env_filter =
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());

        let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);

        let log_path = data_dir.join("daemon.log");

        // Truncate daemon.log on startup so each daemon session starts fresh.
        if log_path.exists() {
            let _ = std::fs::File::create(&log_path);
        }

        // Try to create a SizeManagedWriter for daemon.log. This writer
        // tracks cumulative bytes and automatically drops the oldest 25% of
        // the file when it exceeds 1 GB, keeping the log from growing
        // unbounded. May fail on Windows without admin when data_dir is under
        // a restricted location. Fall back to stderr-only gracefully.
        let writer_result = SizeManagedWriter::new(log_path.clone(), DAEMON_LOG_MAX_BYTES);

        match writer_result {
            Ok(writer) => {
                let (non_blocking, _guard) = tracing_appender::non_blocking(writer);
                let file_layer = tracing_subscriber::fmt::layer()
                    .with_writer(non_blocking)
                    .with_ansi(false);

                let result = tracing_subscriber::registry()
                    .with(env_filter)
                    .with(stderr_layer)
                    .with(file_layer)
                    .try_init();

                if result.is_ok() {
                    tracing::info!("Logging to stderr and {}", log_path.display());
                    tracing::info!("Data directory: {}", data_dir.display());
                }

                // Hold _guard alive for the daemon's entire lifetime.
                std::mem::forget(_guard);
            }
            Err(e) => {
                // File logging unavailable — stderr only
                let result = tracing_subscriber::registry()
                    .with(env_filter)
                    .with(stderr_layer)
                    .try_init();

                if result.is_ok() {
                    tracing::warn!(
                        "Could not open log file {}: {}. Logging to stderr only.",
                        log_path.display(),
                        e
                    );
                }
            }
        }
    }

    // Acquire PID file
    let pid_file_path = data_dir.join("agentcronsystem.pid");
    let pid_file = PidFile::new(pid_file_path);
    pid_file.acquire()?;

    // Initialize WorkflowStore
    let workflow_store = Arc::new(
        crate::storage::workflows::FsWorkflowStore::new(&data_dir).await?,
    ) as Arc<dyn crate::storage::workflows::WorkflowStore>;

    // In-memory workflow runs map.
    let workflow_runs: Arc<RwLock<HashMap<Uuid, Arc<RwLock<crate::models::workflow::WorkflowRun>>>>> =
        Arc::new(RwLock::new(HashMap::new()));

    // WorkflowEvent broadcast channel.
    let (workflow_event_tx, _workflow_event_rx) =
        broadcast::channel::<WorkflowEvent>(config.broadcast_capacity);

    // Scheduler notify — woken whenever the workflow list changes.
    let scheduler_notify = Arc::new(Notify::new());

    // Shutdown channel
    let (shutdown_tx, mut shutdown_rx) = tokio::sync::watch::channel(());

    // Create AppState
    let state = Arc::new(AppState {
        scheduler_notify: Arc::clone(&scheduler_notify),
        config: Arc::clone(&config),
        start_time: Instant::now(),
        shutdown_tx: Some(shutdown_tx.clone()),
        workflow_event_tx: workflow_event_tx.clone(),
        workflow_store,
        workflow_runs,
    });

    // Start Workflow Scheduler
    let wf_sched_clock: Arc<dyn scheduler::Clock> = Arc::new(scheduler::SystemClock);
    let wf_scheduler = scheduler::WorkflowScheduler::new(
        Arc::clone(&state.workflow_store),
        wf_sched_clock,
        Arc::clone(&scheduler_notify),
        workflow_event_tx.clone(),
        Arc::clone(&state.workflow_runs),
        data_dir.clone(),
    );

    let wf_scheduler_handle = tokio::spawn(async move {
        if let Err(e) = wf_scheduler.run().await {
            tracing::error!("Workflow scheduler error: {}", e);
        }
    });

    // Create router and start HTTP server
    let router = server::create_router(Arc::clone(&state));
    let bind_addr = format!("{}:{}", config.host, config.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .context(format!("Failed to bind to {}", bind_addr))?;

    // Write the port file now that we know the actual bound port
    let actual_port = listener.local_addr()?.port();
    let port_file_path = data_dir.join("agentcronsystem.port");
    let port_file = PortFile::write_to(port_file_path, actual_port)?;

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

    // Stop workflow scheduler
    wf_scheduler_handle.abort();

    // Run graceful shutdown sequence (remove PID/port files)
    graceful_shutdown(Some(&pid_file), Some(&port_file)).await;

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
    use tempfile::TempDir;


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
    // 5. Shutdown removes PID and port files
    // =======================================================================
    #[tokio::test]
    async fn test_shutdown_releases_pid_file_simple() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        let pid_file = PidFile::new(pid_path.clone());
        pid_file.acquire().expect("acquire PID file");

        assert!(pid_path.exists(), "PID file should exist before shutdown");

        graceful_shutdown(Some(&pid_file), None).await;

        assert!(!pid_path.exists(), "PID file should be removed after shutdown");
    }

    #[tokio::test]
    async fn test_shutdown_removes_port_file_simple() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let port_path = tmp_dir.path().join("agentcronsystem.port");

        let port_file = PortFile::write_to(port_path.clone(), 8377).expect("write port file");

        assert!(port_path.exists(), "Port file should exist before shutdown");

        graceful_shutdown(None, Some(&port_file)).await;

        assert!(!port_path.exists(), "Port file should be removed after shutdown");
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
    // SizeManagedWriter tests
    // =======================================================================

    #[test]
    fn test_size_managed_writer_tracks_bytes() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        let mut writer = SizeManagedWriter::new(log_path.clone(), 1024).expect("create writer");

        let data = b"hello world\n";
        let n = writer.write(data).expect("write");
        assert_eq!(n, data.len());
        assert_eq!(writer.bytes_written, data.len() as u64);

        writer.flush().expect("flush");
        let content = std::fs::read_to_string(&log_path).expect("read");
        assert_eq!(content, "hello world\n");
    }

    #[test]
    fn test_size_managed_writer_truncates_at_max_size() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        // Use a small max_size so truncation triggers quickly.
        let max_size: u64 = 100;
        let mut writer = SizeManagedWriter::new(log_path.clone(), max_size).expect("create writer");

        // Write 10 lines of 12 bytes each = 120 bytes total, exceeding 100.
        for i in 0..10 {
            writeln!(writer, "line {:05}", i).expect("write line");
        }
        writer.flush().expect("flush");

        let content = std::fs::read_to_string(&log_path).expect("read");
        // After truncation, the oldest 25% should be dropped.
        // The file had 120 bytes; 25% = 30 bytes. The first 3 lines are
        // 36 bytes (3 * 12), so the cut will be after "line 00002\n" at byte 36.
        // The remaining content should start at "line 00003\n".
        assert!(
            !content.contains("line 00000"),
            "Oldest lines should be removed after truncation"
        );
        assert!(
            content.contains("line 00009"),
            "Newest lines should be preserved after truncation"
        );
        // The file should be smaller than the max_size after truncation.
        let file_size = std::fs::metadata(&log_path).expect("metadata").len();
        assert!(
            file_size < max_size,
            "File size ({}) should be less than max_size ({}) after truncation",
            file_size,
            max_size
        );
    }

    #[test]
    fn test_size_managed_writer_newline_alignment() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        let max_size: u64 = 40;
        let mut writer = SizeManagedWriter::new(log_path.clone(), max_size).expect("create writer");

        // Write lines of varying lengths.
        writer.write_all(b"short\n").expect("write");
        writer.write_all(b"medium line\n").expect("write");
        writer.write_all(b"another line here\n").expect("write");
        writer.flush().expect("flush");

        let content = std::fs::read_to_string(&log_path).expect("read");
        // After truncation the content should start at the beginning of a line
        // (i.e., the retained portion should not start mid-line).
        if !content.is_empty() {
            // Verify we didn't cut mid-line: check no partial line at the start
            // by ensuring the content either starts at the first byte or after
            // a newline boundary.
            let lines: Vec<&str> = content.lines().collect();
            assert!(!lines.is_empty(), "Should have at least one complete line");
            // Every line should be one of the known lines (no partial lines).
            for line in &lines {
                assert!(
                    *line == "short" || *line == "medium line" || *line == "another line here",
                    "Found unexpected partial line: '{}'",
                    line
                );
            }
        }
    }

    #[test]
    fn test_size_managed_writer_empty_file_truncation() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        // Create an empty file, then call truncate_oldest_quarter directly.
        let mut writer = SizeManagedWriter::new(log_path.clone(), 100).expect("create writer");
        writer.truncate_oldest_quarter().expect("truncate empty");
        assert_eq!(writer.bytes_written, 0);
    }

    #[test]
    fn test_size_managed_writer_small_file_no_truncation() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        // max_size is large; writes should not trigger truncation.
        let mut writer =
            SizeManagedWriter::new(log_path.clone(), 1_000_000).expect("create writer");
        writer.write_all(b"tiny\n").expect("write");
        writer.flush().expect("flush");

        let content = std::fs::read_to_string(&log_path).expect("read");
        assert_eq!(content, "tiny\n");
        assert_eq!(writer.bytes_written, 5);
    }

    #[test]
    fn test_size_managed_writer_seeds_from_existing_file() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        // Pre-populate the file.
        std::fs::write(&log_path, "existing content\n").expect("write seed");

        let writer = SizeManagedWriter::new(log_path.clone(), 1024).expect("create writer");
        assert_eq!(
            writer.bytes_written, 17,
            "bytes_written should be seeded from the existing file size"
        );
    }

    #[test]
    fn test_size_managed_writer_multiple_truncations() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let log_path = tmp_dir.path().join("daemon.log");

        // Very small max_size to trigger multiple truncations.
        let max_size: u64 = 50;
        let mut writer = SizeManagedWriter::new(log_path.clone(), max_size).expect("create writer");

        // Write enough data to trigger truncation multiple times.
        for i in 0..20 {
            writeln!(writer, "iteration {:04}", i).expect("write");
        }
        writer.flush().expect("flush");

        let content = std::fs::read_to_string(&log_path).expect("read");
        // After multiple truncations, the file should still be valid text
        // with complete lines and be under max_size.
        let file_size = std::fs::metadata(&log_path).expect("metadata").len();
        assert!(
            file_size <= max_size,
            "File ({} bytes) should not exceed max_size ({}) after truncations",
            file_size,
            max_size
        );
        // Should contain some of the latest iterations.
        assert!(
            content.contains("iteration 0019"),
            "Latest data should be present: {}",
            content
        );
    }

    // =======================================================================
    // Failed event sets last_exit_code to -1
    // =======================================================================

}
