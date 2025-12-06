# Agent Cron Scheduler — Development Specification

## 1. Project Overview

**Name**: agent-cron-scheduler
**Binary**: `acs`
**License**: MIT (Jacob Tonna, 2025)
**Language**: Rust (2021 edition, stable toolchain)

A cross-platform cron scheduler that runs as a persistent background daemon with a CLI for management and a web UI for monitoring. Executes shell commands and script files on cron schedules. All process spawning uses PTY emulation for compatibility with programs (such as headless AI agents) that require a terminal environment.

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
│   │  /api/jobs/:id/runs      /api/events (SSE)        │   │
│   │  /api/runs/:id/log       / (static web UI)        │   │
│   └────────┬──────────┬──────────┬───────────────────┘   │
│            │          │          │                        │
│   ┌────────v───┐ ┌────v────┐ ┌──v──────────────┐        │
│   │  Storage   │ │Scheduler│ │ SSE Broadcast    │        │
│   │  (JSON)    │ │ Engine  │ │ (tx/rx)          │        │
│   └────────────┘ └────┬────┘ └──────▲───────────┘        │
│                       │             │                    │
│                  ┌────v─────────────┤                    │
│                  │  Executor (PTY)  │                    │
│                  └──────────────────┘                    │
└──────────────────────────────────────────────────────────┘
```

### 2.2 Event Flow

```
Scheduler tick fires
  → Executor::spawn_job(job)
    → Event Bus: JobEvent::Started
    → PTY spawns process
    → PTY read loop:
        → Event Bus: JobEvent::Output (per chunk)
        → Storage: append_log (per chunk)
    → Process exits
    → Event Bus: JobEvent::Completed or JobEvent::Failed
```

### 2.3 Concurrency Model

- **Scheduler**: Single long-lived `tokio::spawn` task. Computes next wake time, sleeps, dispatches due jobs.
- **Executor**: Each job run spawns a new Tokio task. Multiple jobs run concurrently.
- **Event Bus**: `tokio::sync::broadcast::channel<JobEvent>` (capacity 1024).
- **HTTP Server**: Axum on its own Tokio task. Shared state via `Arc<AppState>`.
- **Notify**: `tokio::sync::Notify` lets the API wake the scheduler when jobs change.

### 2.4 Pattern

Event-driven architecture with actor-like concurrency. The broadcast channel decouples producers (executor) from consumers (SSE, log writer). Adding new consumers requires no changes to the executor.

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
│   │   └── events.rs           # JobEvent enum, broadcast channel
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
│   │   ├── mod.rs
│   │   ├── jobs.rs             # JSON file persistence
│   │   └── logs.rs             # Per-run log file management
│   └── pty/
│       └── mod.rs              # Cross-platform PTY abstraction
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
├── config.json
├── logs/{job_id}/{run_id}.log
├── logs/{job_id}/{run_id}.meta.json
└── scripts/
```

Data directory defaults:
- **Linux**: `~/.local/share/agent-cron-scheduler/`
- **macOS**: `~/Library/Application Support/agent-cron-scheduler/`
- **Windows**: `%APPDATA%\agent-cron-scheduler\`

Config file defaults:
- **Linux**: `~/.config/agent-cron-scheduler/config.json`
- **macOS/Windows**: Same as data dir

---

## 4. Data Models

### Job (`src/models/job.rs`)

```rust
pub enum ExecutionType {
    ShellCommand(String),    // e.g., "echo hello"
    ScriptFile(String),      // relative to {data_dir}/scripts/
}

pub struct Job {
    pub id: Uuid,
    pub name: String,                // unique
    pub schedule: String,            // 5-field cron
    pub execution: ExecutionType,
    pub enabled: bool,
    pub timezone: Option<String>,    // IANA, defaults to UTC
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_exit_code: Option<i32>,
    pub next_run_at: Option<DateTime<Utc>>,  // computed, not persisted
}
```

### JobRun (`src/models/run.rs`)

```rust
pub enum RunStatus { Running, Completed, Failed, Killed }

pub struct JobRun {
    pub run_id: Uuid,
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
pub struct DaemonConfig {
    pub host: String,                  // default: "127.0.0.1"
    pub port: u16,                     // default: 8377
    pub data_dir: Option<PathBuf>,     // default: platform-specific
    pub max_log_files_per_job: usize,  // default: 50
    pub max_log_file_size: u64,        // default: 10MB
    pub max_concurrent_jobs: usize,    // default: 10
    pub default_timeout_secs: u64,     // default: 0 (none)
    pub pty_rows: u16,                 // default: 24
    pub pty_cols: u16,                 // default: 80
}
```

### JobEvent (`src/daemon/events.rs`)

```rust
pub enum JobEvent {
    Started { job_id, run_id, job_name, timestamp },
    Output { job_id, run_id, data: String, timestamp },
    Completed { job_id, run_id, exit_code: i32, timestamp },
    Failed { job_id, run_id, error: String, timestamp },
    JobChanged { job_id, change: JobChangeKind, timestamp },
}

pub enum JobChangeKind { Added, Updated, Removed, Enabled, Disabled }
```

---

## 5. API Specification

Base: `http://127.0.0.1:8377`

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
| PATCH | `/api/jobs/{id}` | Update job (body: JobUpdate) |
| DELETE | `/api/jobs/{id}` | Remove job -> 204 |
| POST | `/api/jobs/{id}/enable` | Enable job |
| POST | `/api/jobs/{id}/disable` | Disable job |
| POST | `/api/jobs/{id}/trigger` | Manual trigger -> 202 |

### Logs

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/jobs/{id}/runs` | List runs (`?limit=20&offset=0&status=`) |
| GET | `/api/runs/{run_id}/log` | Get log output (`?tail=N&format=text`) |

### SSE

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/events` | SSE stream (`?job_id=&run_id=`). Keepalive every 15s. |

### Daemon Control

| Method | Path | Description |
|--------|------|-------------|
| POST | `/api/shutdown` | Graceful shutdown (localhost only, no auth) |

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
```

### Daemon Management
```
acs start [-f|--foreground] [-c|--config PATH] [-p|--port PORT] [--data-dir PATH]
acs stop [--force]
acs status
```

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
| Static files/CORS | `tower-http` 0.6 |
| HTTP client (CLI) | `reqwest` 0.12 |
| CLI parsing | `clap` 4.5 (derive) |
| Cron parsing | `croner` 3 |
| PTY | `portable-pty` 0.9 |
| Serialization | `serde` + `serde_json` |
| Time | `chrono` + `chrono-tz` |
| Tracing | `tracing` + `tracing-subscriber` |
| IDs | `uuid` v4 |
| Errors | `anyhow` + `thiserror` |
| Process info | `sysinfo` |
| Dirs | `dirs` |

> **Note**: The `cron` crate uses 7-field format (incompatible with standard 5-field). `croner` is used instead -- it supports standard 5-field expressions natively.

---

## 8. Cross-Platform Handling

### Process Spawning
- **Windows**: `cmd.exe /C` for shell commands, `powershell.exe -File` for `.ps1`, `cmd.exe /C` for `.bat`/`.cmd`
- **Unix**: `/bin/sh -c` for all

### PTY
- `portable-pty` abstracts ConPTY (Windows, requires version 1809+) and forkpty (Unix)
- Blocking PTY reads use `tokio::task::spawn_blocking` + `mpsc` channel to bridge to async

### Daemonization
- **Unix**: Spawn self with `process_group(0)`, redirect stdio to null
- **Windows**: Spawn with `DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP` creation flags

### Signal Handling
- **Unix**: SIGTERM + SIGINT via `tokio::signal::unix::signal`
- **Windows**: Ctrl+C via `tokio::signal::ctrl_c()`

---

## 9. Implementation Phases

### Phase 1: Foundation
`Cargo.toml`, `src/main.rs`, `src/lib.rs`, `src/models/`, `src/storage/`, `src/errors.rs`
- All data models, JobStore (JSON CRUD with atomic writes), LogStore, config loading, error types
- Unit tests for storage and models

### Phase 2: PTY and Execution
`src/pty/mod.rs`, `src/daemon/executor.rs`, `src/daemon/events.rs`
- Cross-platform PTY abstraction, event bus (broadcast channel), executor with spawn_blocking read loop
- Integration tests for PTY spawning

### Phase 3: Scheduler Engine
`src/daemon/scheduler.rs`
- Sleep/wake loop with `tokio::select!` (sleep vs notify), timezone-aware next-run calculation, concurrent dispatch
- Unit tests for schedule calculation

### Phase 4: HTTP API
`src/server/`
- Axum router, all REST endpoints, SSE streaming, health check, static file serving
- API integration tests

### Phase 5: Daemon Lifecycle
`src/daemon/mod.rs`
- PID file management, daemon spawning (detach), signal handling, graceful shutdown
- Lifecycle tests

### Phase 6: CLI Client
`src/main.rs`, `src/cli/`
- All CLI commands, SSE follow mode, pretty output, job lookup by name/ID
- CLI integration tests via `assert_cmd`

### Phase 7: Web UI
`web/`
- Vanilla HTML/CSS/JS dashboard, job table, log viewer with EventSource, add/edit forms
- Manual testing

### Phase 8: Polish
- Log rotation, timeout enforcement, max concurrent enforcement, corrupted jobs.json recovery
- Full integration test suite, CI pipeline, documentation

---

## 10. Edge Cases

### Scheduler
- **No enabled jobs**: Sleep indefinitely, wake on notify
- **Overlapping run**: Skip, log warning (no queueing)
- **Clock jump forward**: Missed runs not retro-executed
- **Clock jump backward**: Re-sleep to next valid time
- **DST transition**: `chrono-tz` handles correctly
- **Invalid cron in jobs.json**: Skip job, log error, auto-disable
- **Corrupted jobs.json**: Refuse to start, prompt user to fix

### Execution
- **Process hangs**: Kill after `default_timeout_secs` (if > 0)
- **PTY fails to open**: Publish Failed event, don't crash daemon
- **Script not found**: Publish Failed event
- **Non-zero exit**: This is `Completed` (with exit code), not `Failed`. Failed = infrastructure error only.
- **Binary/non-UTF8 output**: Log raw bytes, SSE gets lossy UTF-8
- **Output exceeds max size**: Truncate log, append `[LOG TRUNCATED]` marker, process continues
- **Max concurrent reached**: Queue internally, execute when slot opens

### Storage
- **Concurrent writes**: File locking (flock/LockFileEx) + in-memory RwLock
- **Disk full**: Log error, return 500, don't crash
- **Job deleted while running**: Run continues, logs preserved, cleaned up on next start
- **Duplicate job names**: Reject with 409

### Network
- **Port in use**: Clear error message and exit
- **CLI can't reach daemon**: "Could not connect... Is it running?"
- **SSE client disconnect**: Drop subscriber, no cleanup needed
- **Broadcast backpressure**: Slow subscribers get `Lagged`, skip missed messages

### Daemon
- **Started twice**: Detect via PID file + live process check, refuse
- **Killed by OS**: Stale PID file handled on next start
- **Crash during write**: Atomic write (temp + rename) prevents corruption

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
| `ACS_MAX_LOG_FILES` | max_log_files_per_job | `50` |
| `ACS_MAX_CONCURRENT` | max_concurrent_jobs | `10` |
| `ACS_TIMEOUT` | default_timeout_secs | `0` |
| `ACS_LOG_LEVEL` | tracing filter | `info` |

---

## 12. Testing

### Unit Tests (inline `#[cfg(test)]`)
- Models: serde roundtrip, validation
- Storage: CRUD, atomic write, duplicate detection, empty/missing file
- Scheduler: next-run calculation, timezone, DST
- Events: serialize, broadcast send/receive

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
Runs: `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt -- --check`, release build

---

## 13. Resolved Design Decisions

1. **`croner` over `cron` crate** -- `cron` uses 7-field format, incompatible with standard 5-field
2. **Config location** -- Platform-specific via `dirs`, repo root has `config.example.json` only
3. **`scripts/` and `logs/`** -- Live in `{data_dir}`, not repo root
4. **Script path resolution** -- Relative to `{data_dir}/scripts/`. No `..` traversal. Absolute paths accepted but flagged as non-portable
5. **No auth on shutdown endpoint** -- Localhost-only binding makes this acceptable for single-user tool
6. **Web UI location** -- Resolved from `{exe_dir}/web/` (next to binary)
7. **Job name vs UUID collision** -- Try UUID parse first. Reject job names that are valid UUIDs
8. **SSE in CLI** -- Use `reqwest` streaming with manual SSE parsing
9. **PTY output encoding** -- Raw bytes in log files, lossy UTF-8 for SSE
10. **Blocking PTY reads** -- `spawn_blocking` + `mpsc` channel bridges to async
