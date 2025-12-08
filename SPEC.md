# Agent Cron Scheduler — Development Specification

## 1. Project Overview

**Name**: agent-cron-scheduler
**Binary**: `acs`
**License**: MIT (Jacob Tonna, 2025)
**Language**: Rust (2021 edition, stable toolchain, minimum rustc 1.88)

A cross-platform cron scheduler that runs as a persistent background daemon with a CLI for management and a web UI for monitoring. Executes shell commands and script files on cron schedules. All process spawning uses PTY emulation for compatibility with programs (such as headless AI agents) that require a terminal environment.

On first run, the daemon automatically registers itself as a system service (Windows Service, launchd, or systemd) to ensure persistence across reboots and user logoff.

**Non-goals**: Not agent-specific. No distributed scheduling, multi-node coordination, or cloud deployment.

---

## 2. Architecture

### 2.1 High-Level Diagram

```
┌──────────────────────────────────────────────────────────┐
│                     USER INTERFACES                       │
│   ┌──────────────┐                ┌───────────────────┐  │
│   │  CLI Client   │                │   Web UI (Browser)│  │
│   │  (acs CLI)    │                │  index.html/app.js│  │
│   └──────┬───────┘                └────────┬──────────┘  │
│          │ HTTP requests                   │ HTTP + SSE   │
└──────────┼─────────────────────────────────┼─────────────┘
           │                                 │
           v                                 v
┌──────────────────────────────────────────────────────────┐
│                    DAEMON PROCESS                         │
│                                                          │
│   ┌──────────────────────────────────────────────────┐   │
│   │              Axum HTTP Server                     │   │
│   │  /api/jobs (CRUD)        /health                  │   │
│   │  /api/jobs/{id}/runs     /api/events (SSE)        │   │
│   │  /api/runs/{id}/log      / (static web UI)        │   │
│   └────────┬──────────┬──────────┬───────────────────┘   │
│            │          │          │                        │
│   ┌────────v───┐ ┌────v────┐ ┌──v──────────────┐        │
│   │  Storage   │ │Scheduler│ │ SSE Broadcast    │        │
│   │  (JSON)    │ │ Engine  │ │ (tx/rx)          │        │
│   └────────────┘ └────┬────┘ └──────▲───────────┘        │
│                       │             │                    │
│                  ┌────v─────────────┤                    │
│                  │  Executor (PTY)  │                    │
│                  │       │          │                    │
│                  │  Log Writer <────┘                    │
│                  │  (mpsc channel)                       │
│                  └──────────────────┘                    │
└──────────────────────────────────────────────────────────┘
```

### 2.2 Event Flow

```
Scheduler tick fires
  → Executor::spawn_job(job)
    → Broadcast: JobEvent::Started
    → PTY spawns process
    → PTY read loop (per chunk):
        → Broadcast: JobEvent::Output (via Arc<str>, for SSE consumers)
        → Log Writer mpsc: raw bytes (guaranteed delivery to disk)
    → Process exits
    → Broadcast: JobEvent::Completed or JobEvent::Failed
```

### 2.3 Concurrency Model

- **Scheduler**: Single long-lived `tokio::spawn` task. Computes next wake time, sleeps, dispatches due jobs. Uses `tokio::select!` between sleep and `Notify` for immediate wake on job changes.
- **Executor**: Each job run spawns a new Tokio task. All due jobs run concurrently with no limit. PTY reads use `tokio::task::spawn_blocking` bridged to async via `mpsc` channel.
- **Event Bus**: `tokio::sync::broadcast::channel<JobEvent>` (capacity 4096, configurable). Used for SSE streaming and lifecycle notifications. Output data wrapped in `Arc<str>` to minimize clone cost across subscribers.
- **Log Writer**: Dedicated `tokio::sync::mpsc` channel (bounded, capacity 256) from executor to log writer. Guarantees no log data is lost even if broadcast subscribers lag. Provides natural backpressure to the executor if disk writes are slow.
- **HTTP Server**: Axum on its own Tokio task. Shared state via `Arc<AppState>`.
- **Notify**: `tokio::sync::Notify` lets the API wake the scheduler when jobs are added, modified, or removed.

### 2.4 Pattern

Event-driven architecture with actor-like concurrency. Two channel types serve different purposes:
- **Broadcast** (lossy): For SSE consumers and real-time UI updates. Slow consumers may miss messages.
- **mpsc** (lossless): For log persistence. Data is never dropped.

Adding new consumers requires no changes to the executor.

---

## 3. Project Structure

```
agent-cron-scheduler/
├── Cargo.toml
├── src/
│   ├── main.rs                 # Entry point, clap CLI, command dispatch
│   ├── lib.rs                  # Re-exports for testing
│   ├── errors.rs               # Custom error types
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── jobs.rs             # add, remove, list, enable, disable
│   │   ├── logs.rs             # fetch logs, --follow SSE streaming
│   │   └── daemon.rs           # start, stop, status
│   ├── daemon/
│   │   ├── mod.rs              # Daemon bootstrap, signal handling, PID file
│   │   ├── scheduler.rs        # Cron tick engine
│   │   ├── executor.rs         # PTY process spawning, output capture
│   │   ├── events.rs           # JobEvent enum, broadcast channel
│   │   └── service.rs          # Platform service install/uninstall
│   ├── server/
│   │   ├── mod.rs              # Axum setup, Router, AppState
│   │   ├── routes.rs           # REST endpoint handlers
│   │   ├── sse.rs              # SSE streaming handler
│   │   └── health.rs           # Health check endpoint
│   ├── models/
│   │   ├── mod.rs
│   │   ├── job.rs              # Job, NewJob, JobUpdate
│   │   ├── run.rs              # JobRun, RunStatus
│   │   └── config.rs           # DaemonConfig
│   ├── storage/
│   │   ├── mod.rs              # Storage traits (JobStore, LogStore)
│   │   ├── jobs.rs             # JSON file persistence
│   │   └── logs.rs             # Per-run log file management
│   └── pty/
│       └── mod.rs              # Cross-platform PTY abstraction trait
├── web/
│   ├── index.html
│   ├── style.css
│   └── app.js
├── tests/
│   ├── cli_tests.rs
│   ├── api_tests.rs
│   └── scheduler_tests.rs
├── config.example.json
└── .github/workflows/ci.yml
```

### Runtime Data Directory

Platform-specific (via `dirs` crate), NOT in the repo:

```
{data_dir}/
├── acs.pid
├── jobs.json
├── logs/{job_id}/{run_id}.log       # Raw PTY output bytes
├── logs/{job_id}/{run_id}.meta.json # JobRun metadata
└── scripts/
```

Data directory defaults:
- **Linux**: `~/.local/share/agent-cron-scheduler/`
- **macOS**: `~/Library/Application Support/agent-cron-scheduler/`
- **Windows**: `%APPDATA%\agent-cron-scheduler\`

Config file resolution order:
1. `--config` CLI flag
2. `ACS_CONFIG_DIR` environment variable
3. Platform config dir (`dirs::config_dir()/agent-cron-scheduler/config.json`)
4. Fall back to `{data_dir}/config.json`
5. If no config file exists, use `DaemonConfig::default()`

---

## 4. Data Models

### Job (`src/models/job.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum ExecutionType {
    ShellCommand(String),
    ScriptFile(String),       // relative to {data_dir}/scripts/
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,                            // UUIDv7 (time-sortable)
    pub name: String,                        // unique, must not be a valid UUID
    pub schedule: String,                    // 5-field cron expression
    pub execution: ExecutionType,
    pub enabled: bool,
    pub timezone: Option<String>,            // IANA, defaults to UTC
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_exit_code: Option<i32>,
    #[serde(skip)]
    pub next_run_at: Option<DateTime<Utc>>,  // computed on load, not persisted
}
```

### JobRun (`src/models/run.rs`)

Each run produces a `{run_id}.meta.json` file in `logs/{job_id}/`. For listing with pagination, the daemon reads all `.meta.json` files in the directory, sorts by `started_at` descending, and applies offset/limit in memory. The `max_log_files_per_job` config bounds the total file count.

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RunStatus { Running, Completed, Failed, Killed }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    pub run_id: Uuid,                        // UUIDv7
    pub job_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub log_size_bytes: u64,
    pub error: Option<String>,
}
```

### DaemonConfig (`src/models/config.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub host: String,                  // default: "127.0.0.1"
    pub port: u16,                     // default: 8377
    pub data_dir: Option<PathBuf>,     // default: platform-specific
    pub max_log_files_per_job: usize,  // default: 50
    pub max_log_file_size: u64,        // default: 10MB (10_485_760)
    pub default_timeout_secs: u64,     // default: 0 (no timeout)
    pub broadcast_capacity: usize,     // default: 4096
    pub pty_rows: u16,                 // default: 24
    pub pty_cols: u16,                 // default: 80
}
```

### JobEvent (`src/daemon/events.rs`)

```rust
use std::sync::Arc;

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum JobEvent {
    Started { job_id: Uuid, run_id: Uuid, job_name: String, timestamp: DateTime<Utc> },
    Output { job_id: Uuid, run_id: Uuid, data: Arc<str>, timestamp: DateTime<Utc> },
    Completed { job_id: Uuid, run_id: Uuid, exit_code: i32, timestamp: DateTime<Utc> },
    Failed { job_id: Uuid, run_id: Uuid, error: String, timestamp: DateTime<Utc> },
    JobChanged { job_id: Uuid, change: JobChangeKind, timestamp: DateTime<Utc> },
}

#[derive(Debug, Clone, Serialize)]
pub enum JobChangeKind { Added, Updated, Removed, Enabled, Disabled }
```

### AppState (`src/server/mod.rs`)

```rust
pub struct AppState {
    pub job_store: Arc<dyn JobStore>,
    pub log_store: Arc<dyn LogStore>,
    pub event_tx: broadcast::Sender<JobEvent>,
    pub scheduler_notify: Arc<Notify>,
    pub config: Arc<DaemonConfig>,
    pub start_time: Instant,
    pub active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>>,
}
```

---

## 5. API Specification

Base: `http://127.0.0.1:8377`. Axum 0.8 path syntax: `/{param}` (not `/:param`).

### Health

| Method | Path | Description |
|--------|------|-------------|
| GET | `/health` | Returns status, uptime, active/total jobs, version |

Response:
```json
{
    "status": "ok",
    "uptime_seconds": 3661,
    "active_jobs": 2,
    "total_jobs": 5,
    "version": "0.1.0"
}
```

### Jobs

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/jobs` | List all jobs (optional `?enabled=bool` filter) |
| POST | `/api/jobs` | Create job (body: NewJob) -> 201 |
| GET | `/api/jobs/{id}` | Get single job |
| PATCH | `/api/jobs/{id}` | Update job (body: JobUpdate), validates name uniqueness |
| DELETE | `/api/jobs/{id}` | Remove job -> 204, kills active run if any |
| POST | `/api/jobs/{id}/enable` | Enable job |
| POST | `/api/jobs/{id}/disable` | Disable job |
| POST | `/api/jobs/{id}/trigger` | Manual trigger -> 202 |

### Logs

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/jobs/{id}/runs` | List runs (`?limit=20&offset=0&status=`), sorted by `started_at` desc |
| GET | `/api/runs/{run_id}/log` | Get log output (`?tail=N&format=text`) |

### SSE

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/events` | SSE stream (`?job_id=&run_id=`). Keepalive every 15s. |

SSE event replay is not supported. Clients that reconnect should poll the REST API for missed state.

### Daemon Control

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/shutdown` | Graceful shutdown (localhost only, no auth) |

### Service Management

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/service/status` | Returns service registration status per platform |

### Error Format

All errors: `{ "error": "error_code", "message": "Human-readable description" }`

Status codes: 400 (validation), 404 (not found), 409 (conflict/duplicate name), 422 (semantic error), 500 (internal), 503 (shutting down)

---

## 6. CLI Specification

Binary: `acs`

### Global Options
```
--host <HOST>    Daemon host [default: 127.0.0.1]
--port <PORT>    Daemon port [default: 8377]
-v, --verbose    Verbose output
-V, --version    Print version
-h, --help       Print help
```

### Daemon Management
```
acs start [-f|--foreground] [-c|--config PATH] [-p|--port PORT] [--data-dir PATH]
acs stop [--force]
acs status
acs uninstall     # Remove system service registration
```

`acs start` behavior:
1. Check if daemon is already running (PID file with exclusive create `O_EXCL`/`CREATE_NEW`).
2. If not registered as a system service, auto-register.
3. If `--foreground`, run in current process.
4. Otherwise, start via the system service manager.

`acs uninstall` behavior:
1. Stop the daemon if running.
2. Remove the system service registration.
3. Optionally remove data directory (with `--purge` flag).

### Job Management
```
acs add -n NAME -s "CRON" (-c CMD | --script FILE) [--timezone TZ] [--working-dir PATH] [-e KEY=VALUE]... [--disabled]
acs remove JOB [--yes]
acs list [--enabled] [--disabled] [--json]
acs enable JOB
acs disable JOB
acs trigger JOB [--follow]
```

### Log Access
```
acs logs JOB [--follow] [--run RUN_ID] [--last N] [--tail LINES] [--json]
```

JOB can be a UUID or job name. UUID parse is tried first; if it fails, name lookup is used. Job names that are valid UUIDs are rejected at creation.

---

## 7. Key Crates

| Purpose | Crate |
|---|---|
| Async runtime | `tokio` (full features) |
| HTTP server | `axum` 0.8 |
| Middleware/static files | `tower` 0.5, `tower-http` 0.6 (fs, cors, trace) |
| HTTP client (CLI) | `reqwest` 0.12 (features: json, stream) |
| CLI parsing | `clap` 4.5 (derive) |
| Cron parsing | `croner` 3 |
| PTY | `portable-pty` 0.9 |
| Serialization | `serde` + `serde_json` |
| Time | `chrono` + `chrono-tz` |
| Tracing | `tracing` + `tracing-subscriber` |
| IDs | `uuid` 1.x (features: v7, serde) |
| Errors | `anyhow` + `thiserror` |
| Async traits | `async-trait` |
| Streams | `tokio-stream` (features: sync), `futures-util` |
| File locking | `fs4` |
| Dirs | `dirs` |
| Windows service | `windows-service` (Windows target only) |

> **Notes**:
> - The `cron` crate uses 7-field format (incompatible with standard 5-field). `croner` is used instead.
> - UUIDv7 is time-sortable (embeds millisecond timestamp). Preferred over v4 for chronological ordering.
> - `sysinfo` removed; PID liveness checks use signal-0 (Unix) and `OpenProcess` (Windows) directly.
> - Atomic file writes use `fs4` for locking + write-to-temp + rename pattern.

---

## 8. Cross-Platform Handling

### Process Spawning
- **Windows**: `cmd.exe /C` for shell commands, `powershell.exe -File` for `.ps1`, `cmd.exe /C` for `.bat`/`.cmd`
- **Unix**: `/bin/sh -c` for all (execute permissions not required; scripts invoked via shell)

### PTY
- `portable-pty` abstracts ConPTY (Windows, requires version 1809+) and forkpty (Unix)
- Blocking PTY reads use `tokio::task::spawn_blocking` + `mpsc` channel to bridge to async
- Known ConPTY quirks on Windows: occasional escape sequence noise, race condition on `ClosePseudoConsole`. Mitigated by draining reader before closing.
- Fallback: `--no-pty` mode using `std::process::Command` with piped stdout/stderr for environments where PTY is problematic. Also useful for testing.

### System Service Registration
- **Windows**: Windows Service via `windows-service` crate. Survives user logoff and system reboot.
- **macOS**: launchd plist installed to `~/Library/LaunchAgents/com.acs.scheduler.plist` (user-level) or `/Library/LaunchDaemons/` (system-level with elevated privileges).
- **Linux**: systemd user unit installed to `~/.config/systemd/user/acs.service`. Enabled with `loginctl enable-linger` for persistence without active login session.

### Daemonization (fallback when service manager unavailable)
- **Unix**: Spawn self with `process_group(0)`, redirect stdio to null
- **Windows**: Spawn with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` creation flags
- Note: This is a developer convenience mode only. The service registration path is preferred for production.

### Signal Handling / Graceful Shutdown
- **Unix**: SIGTERM + SIGINT via `tokio::signal::unix::signal`
- **Windows**: Ctrl+C via `tokio::signal::ctrl_c()`, plus service control handler for `SERVICE_CONTROL_STOP`

Shutdown sequence:
1. Stop accepting new HTTP connections
2. Stop scheduling new job runs
3. Send SIGTERM (Unix) / TerminateProcess with grace (Windows) to all running child processes
4. Wait up to 30 seconds for active runs to complete
5. SIGKILL / force-terminate any remaining processes
6. Update all in-flight `JobRun` records to `Killed` status
7. Flush all log files
8. Remove PID file
9. Exit with code 0

### Data Directories
- **Linux**: `~/.local/share/agent-cron-scheduler/`
- **macOS**: `~/Library/Application Support/agent-cron-scheduler/`
- **Windows**: `%APPDATA%\agent-cron-scheduler\`

### Environment Variable Inheritance
Child processes inherit the daemon's environment, with the job's `env_vars` applied as overrides.

---

## 9. Implementation Phases

### Phase 1: Foundation
`Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/models/`, `src/storage/`, `src/errors.rs`

**Tests first (TDD):**
- Job/JobRun/Config serde roundtrip
- Job validation (empty name, UUID-as-name, invalid cron, invalid timezone)
- JobStore CRUD (create, read, update, delete, list, find-by-name)
- JobStore duplicate name rejection
- JobStore atomic writes (write-to-temp + rename)
- LogStore create/append/read log files
- LogStore list runs with pagination
- LogStore cleanup rotation
- DaemonConfig defaults and partial deserialization
- Error type conversions

**Then implement:**
- All data models with serde derives
- `JobStore` trait + JSON file implementation with `fs4` locking
- `LogStore` trait + filesystem implementation
- Config loading with precedence chain
- Custom error types

### Phase 2: PTY and Execution
`src/pty/mod.rs`, `src/daemon/executor.rs`, `src/daemon/events.rs`

**Tests first (TDD):**
- Executor with mock PTY: output "hello\n", exit 0 -> verify Completed event
- Executor with mock PTY: exit 1 -> verify Completed with exit_code=1
- Executor with mock PTY: spawn error -> verify Failed event
- Event ordering: Started before Output before Completed
- Output chunking produces multiple Output events
- Timeout kills process (if timeout > 0)
- JobEvent serde roundtrip
- Broadcast channel: two subscribers both receive event
- Broadcast lagged subscriber receives Lagged error
- Log writer receives all output via mpsc even when broadcast lags

**Then implement:**
- `PtySpawner` trait + real implementation via `portable-pty`
- `PtyProcess` trait for testability
- `MockPtySpawner` / `MockPtyProcess` test doubles
- Event bus setup (broadcast channel)
- Log writer task (mpsc consumer)
- Executor with spawn_blocking PTY read loop

### Phase 3: Scheduler Engine
`src/daemon/scheduler.rs`

**Tests first (TDD):**
- next_run_at for `*/5 * * * *` at 10:03 -> 10:05
- next_run_at for `0 0 * * *` with timezone offset
- DST spring-forward: 2:30 AM skipped
- DST fall-back: first occurrence used
- No enabled jobs -> sleeps indefinitely until notified
- Scheduler dispatches job when cron fires (using FakeClock)
- Scheduler skips disabled jobs
- Scheduler dispatches all due jobs concurrently (no limiting)
- Scheduler wakes on Notify when job added/updated/removed
- Invalid cron expression -> job auto-disabled, error logged
- Timezone revalidated on each tick

**Then implement:**
- `Clock` trait + `SystemClock` / `FakeClock` implementations
- Scheduler loop with `tokio::select!`
- Timezone-aware next-run calculation via `croner` + `chrono-tz`

### Phase 4: HTTP API
`src/server/`

**Tests first (TDD):**
- GET /health returns 200 with all expected fields
- POST /api/jobs with valid body returns 201
- POST /api/jobs with invalid cron returns 400
- POST /api/jobs with duplicate name returns 409
- POST /api/jobs with UUID-like name returns 400
- GET /api/jobs returns all jobs
- GET /api/jobs?enabled=true filters correctly
- GET /api/jobs/{id} returns 404 for unknown ID
- PATCH /api/jobs/{id} updates fields, validates name uniqueness
- DELETE /api/jobs/{id} returns 204
- POST /api/jobs/{id}/trigger returns 202
- GET /api/jobs/{id}/runs with pagination
- GET /api/runs/{run_id}/log returns log text
- GET /api/events SSE stream emits events
- POST /api/shutdown initiates shutdown
- All error responses match documented format

**Then implement:**
- Axum router with shared AppState (using trait objects for testability)
- All REST endpoint handlers
- SSE streaming handler (subscribe to broadcast, filter by query params)
- Health check endpoint
- Static file serving from `web/` directory

### Phase 5: Daemon Lifecycle + Service Registration
`src/daemon/mod.rs`, `src/daemon/service.rs`

**Tests first (TDD):**
- PidFile acquire creates file (exclusive create)
- PidFile acquire fails if already held by live process
- PidFile acquire succeeds if PID file is stale
- PidFile release removes file
- Shutdown sequence marks running jobs as Killed
- Service detection (is service registered?)

**Then implement:**
- PidFile struct with acquire/release/is_alive
- Daemon spawning (service manager or fallback detach)
- Signal handling
- Graceful shutdown sequence
- Platform service install/uninstall (Windows Service, launchd, systemd)

### Phase 6: CLI Client
`src/main.rs`, `src/cli/`

**Tests first (TDD):**
- `acs --version` prints version
- `acs add` with valid args (via `assert_cmd`)
- `acs list --json` outputs valid JSON array
- `acs remove --yes` skips confirmation
- Job lookup: UUID string resolved by ID, non-UUID resolved by name
- `acs status` when daemon not running shows error

**Then implement:**
- Clap derive CLI definition with all commands
- All command handlers (HTTP client calls to daemon)
- SSE follow mode via reqwest streaming
- Pretty table output for list
- Job lookup by name or ID

### Phase 7: Web UI
`web/`
- Vanilla HTML/CSS/JS dashboard
- Job table: name, schedule, status, last run, next run, actions
- Log viewer with live streaming via EventSource
- Add/edit job forms (modal)
- Health status indicator
- Manual testing

### Phase 8: Polish
- Log rotation (enforce `max_log_files_per_job`)
- Timeout enforcement
- Corrupted jobs.json recovery (backup + warning)
- Orphaned log cleanup on startup (logs for deleted jobs)
- Broadcast capacity load testing and tuning
- Comprehensive integration test suite
- CI pipeline with coverage enforcement
- Documentation (README, architecture guide)

---

## 10. Edge Cases

### Scheduler
- **No enabled jobs**: Sleep indefinitely, wake only on Notify
- **Multiple jobs due simultaneously**: All dispatched concurrently, no limiting
- **Clock jump forward**: Missed runs not retro-executed; compute next future run
- **Clock jump backward**: Re-sleep to next valid time; no duplicate runs
- **DST transition**: `chrono-tz` handles correctly; jobs at nonexistent times fire at next valid time
- **Invalid cron in jobs.json**: Skip job, log error, auto-disable
- **Invalid timezone in jobs.json**: Skip job, log error, auto-disable. Revalidated each tick.
- **Corrupted jobs.json**: Refuse to start, prompt user to fix or delete

### Execution
- **Process hangs**: Kill after `default_timeout_secs` (if > 0); publish Failed with "execution timed out"
- **PTY fails to open**: Publish Failed event, don't crash daemon
- **Script not found**: Publish Failed event with clear error
- **Non-zero exit**: This is `Completed` (with exit code), not `Failed`. Failed = infrastructure error only.
- **Binary/non-UTF8 output**: Raw bytes in log files, lossy UTF-8 (`Arc<str>`) for broadcast/SSE
- **Output exceeds max size**: Truncate log file, append `[LOG TRUNCATED]` marker, process continues
- **Working directory doesn't exist**: Validated at execution time (not creation). Publish Failed if invalid.

### Storage
- **Concurrent writes**: File locking via `fs4` + in-memory `RwLock`. Atomic write: write to `.tmp`, then rename.
- **Atomic rename on Windows**: `fs4` handles cross-platform rename. Retry on `ERROR_ACCESS_DENIED`.
- **Disk full**: Log error, return 500, don't crash
- **Job deleted while running**: Run continues, logs preserved, orphaned logs cleaned up on next startup
- **Duplicate job names**: Reject with 409. Also validated on PATCH (update) excluding self.
- **Large number of jobs**: Design targets <1000 jobs in single JSON file. SQLite migration path documented for future.

### Network
- **Port in use**: Clear error message and exit
- **CLI can't reach daemon**: "Could not connect to daemon at {host}:{port}. Is it running? (try: acs start)"
- **SSE client disconnect**: Drop broadcast subscriber, no cleanup needed
- **Broadcast backpressure**: Slow subscribers receive `Lagged`, skip missed messages. SSE handler sends a comment to inform the client.

### Daemon
- **Started twice**: PID file uses exclusive create (`O_EXCL`/`CREATE_NEW`). If file exists, check if PID is alive. If alive, refuse. If stale, remove and proceed.
- **Killed by OS**: Stale PID file handled on next start
- **Crash during write**: Atomic write (temp + rename) prevents corruption
- **User logoff (Windows)**: Service registration ensures daemon survives logoff. Without service, `DETACHED_PROCESS` is killed.

---

## 11. Configuration

### Precedence (highest to lowest)
1. CLI flags (`--port 9000`)
2. Environment variables (`ACS_PORT=9000`)
3. Config file (`config.json`)
4. Compiled defaults

### Environment Variables

| Variable | Setting | Default |
|---|---|---|
| `ACS_HOST` | host | `127.0.0.1` |
| `ACS_PORT` | port | `8377` |
| `ACS_DATA_DIR` | data_dir | platform default |
| `ACS_CONFIG_DIR` | config file directory | platform default |
| `ACS_MAX_LOG_FILES` | max_log_files_per_job | `50` |
| `ACS_TIMEOUT` | default_timeout_secs | `0` |
| `ACS_BROADCAST_CAPACITY` | broadcast_capacity | `4096` |
| `ACS_LOG_LEVEL` | tracing filter | `info` |

---

## 12. Testing

### Methodology: Test-Driven Development (TDD)

All code is written test-first. For each implementation phase, tests are written before the production code. The test suite targets:
- **Line coverage >= 90%** across the codebase
- **Branch coverage >= 85%**
- Excluded from metrics: `web/` directory, `main()` function, platform-specific FFI that cannot be tested on CI without the target platform

### Trait Abstractions for Testability

```rust
#[async_trait]
pub trait JobStore: Send + Sync {
    async fn list_jobs(&self) -> Result<Vec<Job>>;
    async fn get_job(&self, id: Uuid) -> Result<Option<Job>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Job>>;
    async fn create_job(&self, new: NewJob) -> Result<Job>;
    async fn update_job(&self, id: Uuid, update: JobUpdate) -> Result<Job>;
    async fn delete_job(&self, id: Uuid) -> Result<()>;
}

#[async_trait]
pub trait LogStore: Send + Sync {
    async fn create_run(&self, run: &JobRun) -> Result<()>;
    async fn update_run(&self, run: &JobRun) -> Result<()>;
    async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> Result<()>;
    async fn read_log(&self, job_id: Uuid, run_id: Uuid, tail: Option<usize>) -> Result<String>;
    async fn list_runs(&self, job_id: Uuid, limit: usize, offset: usize) -> Result<(Vec<JobRun>, usize)>;
    async fn cleanup(&self, job_id: Uuid, max_files: usize) -> Result<()>;
}

pub trait PtySpawner: Send + Sync {
    fn spawn(&self, cmd: CommandBuilder, rows: u16, cols: u16) -> Result<Box<dyn PtyProcess>>;
}

pub trait PtyProcess: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn kill(&mut self) -> io::Result<()>;
    fn wait(&mut self) -> io::Result<ExitStatus>;
}

pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
```

Test doubles: `InMemoryJobStore`, `InMemoryLogStore`, `MockPtySpawner`, `FakeClock`.

### Coverage Tool

**`cargo-llvm-cov`** — works on Linux, macOS, and Windows (unlike tarpaulin which is Linux-only). Produces LCOV output for CI integration with Codecov.

```sh
# Local
cargo llvm-cov --html --open

# CI
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

### Unit Tests (inline `#[cfg(test)]`)
- Models: serde roundtrip, validation, edge cases
- Storage: CRUD, atomic write, duplicate detection, empty/missing file, pagination
- Scheduler: next-run calculation, timezone, DST, notify wake
- Events: serialize, broadcast send/receive, Arc<str> cloning

### Integration Tests (`tests/`)
- `api_tests.rs`: All HTTP endpoints, error responses, SSE connection
- `cli_tests.rs`: Command parsing, output formatting (via `assert_cmd`)
- `scheduler_tests.rs`: End-to-end create -> schedule -> run -> verify log

### CI (GitHub Actions)
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, windows-latest, macos-latest]
    rust: [stable]
```
Runs: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt -- --check`, `cargo llvm-cov`, release build.
Coverage enforced at 90% minimum via CI.

---

## 13. Resolved Design Decisions

1. **`croner` over `cron` crate** -- `cron` uses 7-field format, incompatible with standard 5-field
2. **UUIDv7 over v4** -- Time-sortable IDs; chronological ordering for free on job runs
3. **Unlimited concurrency** -- When a cron fires, the job runs. No skipping, no queueing, no concurrency limits.
4. **Dual-channel architecture** -- Broadcast (lossy) for SSE/UI, mpsc (lossless) for log persistence
5. **`Arc<str>` in broadcast events** -- Cheap cloning across multiple SSE subscribers
6. **Auto service registration** -- `acs start` auto-installs as system service on first run. Supports Windows Service, launchd, and systemd.
7. **Config location** -- Platform-specific via `dirs` with documented resolution chain
8. **`scripts/` and `logs/`** -- Live in `{data_dir}`, not repo root
9. **Script path resolution** -- Relative to `{data_dir}/scripts/`. No `..` traversal. Absolute paths accepted but flagged as non-portable
10. **No auth on shutdown endpoint** -- Localhost-only binding makes this acceptable for single-user tool
11. **Web UI location** -- Resolved from `{exe_dir}/web/` with fallback to `./web/` for development
12. **Job name vs UUID collision** -- Try UUID parse first. Reject job names that are valid UUIDs
13. **SSE in CLI** -- Use `reqwest` streaming with manual SSE parsing
14. **PTY output encoding** -- Raw bytes in log files, lossy UTF-8 for SSE
15. **Blocking PTY reads** -- `spawn_blocking` + `mpsc` channel bridges to async
16. **PTY fallback** -- `--no-pty` mode for environments where PTY is problematic and for testing
17. **Atomic writes** -- `fs4` for file locking + write-to-temp + rename pattern
18. **PID file locking** -- Exclusive create prevents race conditions on concurrent start
19. **Graceful shutdown** -- SIGTERM -> 30s grace -> SIGKILL -> mark Killed -> cleanup
20. **TDD with cargo-llvm-cov** -- 90% line coverage, 85% branch coverage, cross-platform
21. **Environment inheritance** -- Child processes inherit daemon environment, job `env_vars` applied as overrides
