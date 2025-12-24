# Architecture: Agent Cron Scheduler (ACS)

This document describes the architecture of the Agent Cron Scheduler as it exists
in the codebase. It is derived entirely from source code review, not from spec
documents or aspirational design. Where the code differs from common expectations
or contains notable design choices, those are called out explicitly.

---

## 1. System Overview

ACS is a single-binary Rust application (binary name: `acs`, crate name:
`agent_cron_scheduler`) that functions as a local cron daemon with an HTTP API, a
CLI client, real-time event streaming via SSE, and a browser-based dashboard. It
runs as a long-lived background process, scheduling and executing shell commands
or script files on cron schedules with optional timezone awareness.

The binary serves dual roles: it is both the daemon (via `acs start`) and the CLI
client (via `acs add`, `acs trigger`, `acs logs`, etc.). The CLI communicates
with a running daemon over HTTP on `127.0.0.1:8377` by default.

**Key characteristics:**
- Single binary, no external database -- all persistence is via JSON files and
  per-run log files on the local filesystem.
- UUID v7 (time-ordered) identifiers for all entities.
- Cross-platform: Windows, macOS, and Linux, with platform-specific service
  registration.
- Async throughout, built on Tokio.
- Version: 0.1.0

---

## 2. Module Structure

```
src/
  main.rs           -- Entry point: clap parse + tracing init + cli::dispatch
  lib.rs             -- Module declarations
  errors.rs          -- AcsError enum (thiserror)
  models/
    mod.rs           -- Re-exports
    job.rs           -- Job, NewJob, JobUpdate, ExecutionType, validation fns
    run.rs           -- JobRun, RunStatus
    config.rs        -- DaemonConfig
  storage/
    mod.rs           -- JobStore + LogStore trait definitions
    jobs.rs          -- JsonJobStore (file-backed, in-memory cache)
    logs.rs          -- FsLogStore (filesystem per-run logs)
  daemon/
    mod.rs           -- PidFile, config loading, data dir resolution, bootstrap,
                        graceful shutdown, orphan cleanup
    scheduler.rs     -- Clock trait, compute_next_run, Scheduler loop
    executor.rs      -- Executor, RunHandle, command building, spawn lifecycle
    events.rs        -- JobEvent enum, JobChangeKind enum
    service.rs       -- Platform service registration (Windows/macOS/Linux)
  server/
    mod.rs           -- AppState, create_router, inline integration tests
    routes.rs        -- All HTTP route handlers
    sse.rs           -- SSE endpoint handler
    health.rs        -- Health check handler
  cli/
    mod.rs           -- Clap CLI definition, dispatch function
    jobs.rs          -- Job CRUD CLI commands + SSE follow for trigger
    logs.rs          -- Log viewing CLI commands + SSE follow
    daemon.rs        -- Daemon start/stop/status/uninstall commands
  pty/
    mod.rs           -- PtySpawner + PtyProcess traits, NoPtySpawner,
                        MockPtySpawner
frontend/            -- Next.js frontend (source)
  src/app/           -- App Router pages and layouts
  next.config.ts     -- Static export configuration
  package.json       -- Frontend dependencies
web/                 -- Build artifact (generated from frontend/ by build.rs)
web.backup/          -- Original vanilla JS/CSS/HTML frontend (preserved for reference)
tests/
  api_tests.rs       -- HTTP integration tests (real server, reqwest)
  cli_tests.rs       -- Binary integration tests (assert_cmd)
  scheduler_tests.rs -- End-to-end scheduler/executor tests (tempdir + real FsLogStore)
```

---

## 3. Data Flow

### Job Creation Flow

```
CLI (acs add) or Web UI
  --> POST /api/jobs
    --> validate_new_job() -- cron, timezone, name constraints
    --> find_by_name() -- duplicate check
    --> job_store.create_job() -- persist to jobs.json
    --> broadcast JobEvent::JobChanged(Added)
    --> scheduler_notify.notify_one() -- wake scheduler
    <-- 201 Created + Job JSON
```

### Scheduled Execution Flow

```
Scheduler::run() loop
  --> load enabled jobs from job_store
  --> compute_next_run() for each (timezone-aware via croner + chrono-tz)
  --> sleep until earliest, OR wake on Notify
  --> dispatch due Job(s) over mpsc channel
    --> Dispatch loop receives Job
      --> Executor::spawn_job()
        --> create JobRun (Running) in log_store
        --> broadcast JobEvent::Started
        --> PtySpawner::spawn() -- build command, start process
        --> spawn_blocking: read loop (8192-byte buffer)
        --> forward output to:
            1. broadcast channel (JobEvent::Output with Arc<str>)
            2. mpsc -> log writer task -> log_store.append_log()
        --> on completion/timeout/kill:
            --> update JobRun status
            --> broadcast Completed/Failed
            --> log_store.cleanup() -- remove oldest runs
      --> RunHandle stored in active_runs map
  --> Job metadata updater task listens for Completed/Failed events
      --> updates last_run_at, last_exit_code on the Job via job_store.update_job()
```

### Manual Trigger Flow

```
CLI (acs trigger) or Web UI
  --> POST /api/jobs/{id}/trigger
    --> resolve_job (UUID or name)
    --> send Job over dispatch_tx (mpsc)
    <-- 202 Accepted
  --> same Executor::spawn_job() path as scheduled execution
```

### SSE Event Flow

```
broadcast::channel<JobEvent>
  --> Executor broadcasts: Started, Output, Completed, Failed
  --> Route handlers broadcast: JobChanged (Added/Updated/Removed/Enabled/Disabled)
  --> SSE clients: BroadcastStream -> filter by job_id/run_id -> SSE Event
  --> Web dashboard: EventSource -> update job table / stream logs
  --> CLI --follow: reqwest streaming -> parse SSE text protocol
```

---

## 4. Process Model

ACS runs as a single process containing several concurrent Tokio tasks:

| Task                  | Type            | Purpose                                          |
|-----------------------|-----------------|--------------------------------------------------|
| HTTP server           | tokio::spawn    | Axum server, graceful shutdown via watch channel  |
| Scheduler             | tokio::spawn    | Cron loop: sleep + dispatch                      |
| Dispatch loop         | tokio::spawn    | Receives Jobs from scheduler/API, calls Executor |
| Job metadata updater  | tokio::spawn    | Listens for Completed/Failed, updates job store   |
| Per-run execution     | tokio::spawn    | One per active job run, manages PTY read + log    |
| Per-run PTY reader    | spawn_blocking  | Blocking read loop on PTY/pipe output             |
| Per-run log writer    | tokio::spawn    | Receives output via mpsc, appends to log file     |

**Concurrency primitives used:**
- `broadcast::channel<JobEvent>` (capacity 4096) -- fan-out events to SSE clients,
  metadata updater, and CLI follow mode.
- `mpsc::channel<Job>` (capacity 64) -- scheduler and API trigger send jobs to the
  dispatch loop.
- `mpsc::channel<Vec<u8>>` (capacity 256) -- per-run output forwarding to log writer.
- `oneshot::channel<()>` -- per-run kill signal.
- `watch::channel<()>` -- daemon shutdown signal.
- `Notify` -- scheduler wake-up on job list changes.
- `RwLock<HashMap<Uuid, RunHandle>>` -- active runs map shared between routes and
  shutdown.
- `RwLock<Vec<Job>>` -- in-memory job cache in JsonJobStore.

**Single-instance enforcement:** A PID file (`acs.pid`) is created with exclusive
file creation (`create_new(true)` / `O_EXCL`). If the file exists, the recorded
PID is checked via platform-specific liveness detection (Unix: `kill(pid, 0)`;
Windows: `OpenProcess` FFI). Stale PID files are automatically removed.

---

## 5. Storage Layer

### JobStore Trait

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
```

**JsonJobStore** (`storage/jobs.rs`):
- Backing file: `{data_dir}/jobs.json`
- In-memory cache: `RwLock<Vec<Job>>`
- Write strategy: atomic write via `.json.tmp` then rename.
- Corruption recovery: if `jobs.json` fails to parse, it is renamed to `.bak`
  and the store starts empty. This is a data-loss-preventing fallback.
- Duplicate name check is enforced at create time.
- All identifiers are UUID v7.

### LogStore Trait

```rust
#[async_trait]
pub trait LogStore: Send + Sync {
    async fn create_run(&self, run: &JobRun) -> Result<()>;
    async fn update_run(&self, run: &JobRun) -> Result<()>;
    async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> Result<()>;
    async fn read_log(&self, job_id: Uuid, run_id: Uuid, tail: Option<usize>) -> Result<String>;
    async fn list_runs(&self, job_id: Uuid, limit: usize, offset: usize) -> Result<(Vec<JobRun>, usize)>;
    async fn cleanup(&self, job_id: Uuid, max_files: usize) -> Result<()>;
}
```

**FsLogStore** (`storage/logs.rs`):
- Directory structure: `{data_dir}/logs/{job_id}/{run_id}.meta.json` + `{run_id}.log`
- `append_log` opens files with `create(true).append(true)` on each call.
- `read_log` supports an optional `tail` parameter for reading the last N lines.
- `list_runs` reads all `.meta.json` files for a job, sorts by `started_at`
  descending, and paginates.
- `cleanup` removes the oldest run files (both `.meta.json` and `.log`) when the
  count exceeds `max_log_files_per_job` (default 50).

**Design note:** Both traits use `anyhow::Result` rather than a custom error type.
The `AcsError` enum defined in `errors.rs` is used for validation (in the models
layer) but not in storage trait signatures. This is a pragmatic choice that
simplifies storage implementations at the cost of less precise error typing at
trait boundaries.

---

## 6. Scheduler Engine

File: `daemon/scheduler.rs`

### Clock Abstraction

```rust
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}
```

Two implementations:
- **SystemClock** -- production, delegates to `Utc::now()`.
- **FakeClock** -- testing, uses `Arc<std::sync::RwLock<DateTime<Utc>>>`.
  Uses `std::sync::RwLock` (not `tokio::sync::RwLock`) intentionally so it can be
  called from both sync and async contexts.

### compute_next_run

```rust
pub fn compute_next_run(
    schedule: &str,
    timezone: Option<&str>,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>>
```

- Parses cron via `croner::Cron::from_str`.
- If timezone is provided, converts `after` to the target timezone, finds the next
  occurrence in that zone via `find_next_occurrence`, and converts back to UTC.
- The `after` parameter is exclusive -- if `after` is exactly on a cron boundary,
  the result is the *next* boundary.
- DST handling is delegated to `croner` + `chrono-tz`. Tests verify both
  spring-forward (skipped time) and fall-back (ambiguous time) scenarios.

### Scheduler Loop

The `Scheduler::run()` method is an infinite loop:

1. Load all jobs from the job store.
2. Filter to enabled jobs.
3. Compute `next_run` for each. Invalid cron expressions are logged and skipped
   (the job is never dispatched, but the scheduler continues).
4. If no enabled jobs exist, sleep indefinitely on `Notify`.
5. Otherwise, sleep until the earliest `next_run`, using `tokio::select!` between
   the sleep future and `notify.notified()`.
6. On wake from sleep: dispatch all due jobs (those whose `next_run <= now`) via
   the `mpsc::Sender<Job>`.
7. On wake from notify: loop back to step 1 (re-evaluate the job list).

**Design note:** The scheduler dispatches jobs over an mpsc channel rather than
executing them directly. This decouples scheduling from execution and allows the
API trigger endpoint to use the same dispatch channel.

---

## 7. Executor

File: `daemon/executor.rs`

### Command Building

`Executor::build_command(job)` constructs a `portable_pty::CommandBuilder`:

| Execution Type     | Windows                           | Unix             |
|--------------------|-----------------------------------|------------------|
| ShellCommand(cmd)  | `cmd.exe /C {cmd}`                | `/bin/sh -c {cmd}` |
| ScriptFile(path)   | `powershell.exe -File {path}` for `.ps1`, otherwise `cmd.exe /C {path}` | `/bin/sh {path}` |

Working directory and environment variables from the Job are applied to the command.

### Spawn Lifecycle

`Executor::spawn_job(&self, job: &Job) -> Result<RunHandle>`:

1. Generate UUID v7 for `run_id`.
2. Create `JobRun` with `Running` status in log store.
3. Broadcast `JobEvent::Started`.
4. Build command and spawn via `PtySpawner`.
5. If spawn fails: broadcast `Failed`, update run to `Failed`, call cleanup, return.
6. Create mpsc channel (capacity 256) for log writing.
7. Spawn log writer task (receives bytes, appends to log store).
8. Spawn blocking PTY read loop (8192-byte buffer, reads until EOF or error).
9. Forward output chunks from blocking reader to broadcast (as `Arc<str>`) and to
   log writer (as raw bytes).
10. `tokio::select!` on: output chunks, kill signal (oneshot), timeout future.
11. On completion: determine exit status.
12. On timeout: mark as `Failed` with "execution timed out".
13. On kill: mark as `Killed` with "Job was killed".
14. After any terminal state: drop log writer channel, wait for reader join handle,
    call `log_store.cleanup()`.

**Exit status semantics:** A non-zero exit code results in `RunStatus::Completed`
(not `Failed`). The `exit_code` field records the actual code. `RunStatus::Failed`
is reserved for infrastructure errors (spawn failure, PTY errors, timeout, task
join errors). This is a deliberate design choice documented in the code comments.

### Timeout Handling

- If `job.timeout_secs > 0`, that value is used.
- Otherwise, `config.default_timeout_secs` is used.
- If both are 0, no timeout is applied (the sleep is set to ~136 years:
  `Duration::from_secs(u64::MAX / 2)`).

### RunHandle

```rust
pub struct RunHandle {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub join_handle: JoinHandle<()>,
    pub kill_tx: oneshot::Sender<()>,
}
```

Active `RunHandle`s are stored in `AppState::active_runs` (`RwLock<HashMap<Uuid, RunHandle>>`),
keyed by `job_id`. This means only one run per job can be tracked at a time in the
active runs map. If a job is triggered while already running, the new run is added
to the map, replacing the previous handle (though the previous task continues
running in the background).

---

## 8. Event System

File: `daemon/events.rs`

### JobEvent

```rust
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum JobEvent {
    Started { job_id, run_id, job_name, timestamp },
    Output  { job_id, run_id, data: Arc<str>, timestamp },
    Completed { job_id, run_id, exit_code, timestamp },
    Failed  { job_id, run_id, error: String, timestamp },
    JobChanged { job_id, change: JobChangeKind, timestamp },
}
```

### JobChangeKind

```rust
pub enum JobChangeKind {
    Added, Updated, Removed, Enabled, Disabled,
}
```

**Output event optimization:** The `data` field in `Output` uses `Arc<str>` rather
than `String`. This allows the broadcast channel (which clones the event for each
subscriber) to share the underlying string allocation. A custom serializer
(`serialize_arc_str`) ensures it serializes as a plain JSON string. Tests verify
that `Arc::ptr_eq` holds across clones.

**Event producers:**
- **Executor**: `Started`, `Output`, `Completed`, `Failed`
- **Route handlers**: `JobChanged` (on create, update, delete, enable, disable)

**Event consumers:**
- **SSE handler**: all event types, filtered per-client.
- **Job metadata updater task**: `Completed` and `Failed` (updates `last_run_at`
  and `last_exit_code` on the Job).
- **CLI follow mode**: `Started`, `Output`, `Completed`, `Failed`.
- **Web dashboard**: all event types via EventSource.

---

## 9. HTTP API

Framework: Axum 0.8

### Routes

| Method | Path                      | Handler           | Status Codes       |
|--------|---------------------------|-------------------|--------------------|
| GET    | /health                   | health_check      | 200                |
| GET    | /api/jobs                 | list_jobs         | 200, 500           |
| POST   | /api/jobs                 | create_job        | 201, 400, 409, 500 |
| GET    | /api/jobs/{id}            | get_job           | 200, 404, 500      |
| PATCH  | /api/jobs/{id}            | update_job        | 200, 400, 404, 409, 500 |
| DELETE | /api/jobs/{id}            | delete_job        | 204, 404, 500      |
| POST   | /api/jobs/{id}/enable     | enable_job        | 200, 404, 500      |
| POST   | /api/jobs/{id}/disable    | disable_job       | 200, 404, 500      |
| POST   | /api/jobs/{id}/trigger    | trigger_job       | 202, 404, 500      |
| GET    | /api/jobs/{id}/runs       | list_runs         | 200, 404, 500      |
| GET    | /api/runs/{run_id}/log    | get_log           | 200, 400, 404, 500 |
| GET    | /api/events               | sse_handler       | 200 (streaming)    |
| POST   | /api/shutdown             | shutdown          | 200                |
| GET    | /api/service/status       | service_status    | 200                |
| *      | /*                        | ServeDir(web/)    | (static files)     |

### Job Resolution

The `{id}` path parameter in job routes accepts either a UUID or a job name.
`resolve_job()` first attempts `Uuid::parse_str`; if that succeeds, it looks up
by UUID. If not, it falls back to `find_by_name()`. This is why job names are
prohibited from being valid UUIDs (enforced by validation).

### Query Parameters

- `GET /api/jobs?enabled={bool}` -- filter by enabled status.
- `GET /api/jobs/{id}/runs?limit={n}&offset={n}&status={str}` -- pagination and
  status filter (default limit: 20).
- `GET /api/runs/{run_id}/log?tail={n}` -- last N lines.

### Computed Fields

`next_run_at` is not persisted (it is `#[serde(skip_deserializing, default)]`
on the Job struct). Both `list_jobs` and `get_job` handlers compute it on-the-fly
by calling `compute_next_run()` for each enabled job before serializing the response.

### Error Response Format

All error responses follow the structure:
```json
{ "error": "not_found", "message": "Job with id '...' not found" }
```

Error codes used: `not_found`, `conflict`, `validation_error`, `internal_error`.

### Log Retrieval Design

`GET /api/runs/{run_id}/log` does not require a `job_id` parameter. Instead, it
iterates over all jobs and tries `read_log(job.id, run_id, tail)` for each until
it finds a non-empty result. This is an O(n) scan over jobs but avoids requiring
clients to know the job-to-run mapping.

### Static File Serving

The router falls back to `tower_http::services::ServeDir` for the `web/` directory.
The web directory is resolved by checking `{exe_dir}/web/` first, then `./web/`.

---

## 10. SSE Streaming

File: `server/sse.rs`

### Connection

`GET /api/events?job_id={uuid}&run_id={uuid}`

Both query parameters are optional filters. If provided, only events matching the
specified job and/or run are forwarded to the client.

### Implementation

- Subscribes to the broadcast channel via `event_tx.subscribe()`.
- Wraps the receiver in `BroadcastStream` and applies `filter_map`:
  1. Apply `job_id` filter (extracts job_id from each event variant).
  2. Apply `run_id` filter (extracts run_id; `JobChanged` has no run_id, so it
     passes the run_id filter by default when no run_id is present on the event).
  3. Serialize to JSON and wrap in an SSE `Event` with the event type name
     (`started`, `output`, `completed`, `failed`, `job_changed`).
- Lagged subscribers (who fall behind the broadcast capacity) receive an SSE
  comment: `"lagged: some events were missed"`.
- Keepalive: every 15 seconds, text `"keepalive"`.
- A `SseDropGuard` struct logs at debug level when the SSE stream is dropped
  (client disconnects). The guard is moved into the filter_map closure and
  referenced with `let _ = &_drop_guard` to prevent premature optimization.

---

## 11. CLI Client

File: `cli/mod.rs`, `cli/jobs.rs`, `cli/logs.rs`, `cli/daemon.rs`

Framework: Clap 4.5 with derive macros.

### Global Options

- `--host` (default: `127.0.0.1`)
- `--port` (default: `8377`)
- `--verbose` (enables tracing)

### Subcommands

| Command   | Description                           |
|-----------|---------------------------------------|
| start     | Start the daemon (-f foreground, -c config, -p port, --data-dir) |
| stop      | Stop a running daemon (--force placeholder) |
| status    | Show daemon health info               |
| uninstall | Uninstall service registration (--purge placeholder) |
| add       | Create a new job                      |
| remove    | Delete a job (with --yes for non-interactive) |
| list      | List jobs (--enabled/--disabled/--json) |
| enable    | Enable a job                          |
| disable   | Disable a job                         |
| trigger   | Trigger immediate execution (--follow for live output) |
| logs      | View job logs (--follow, --run, --last, --tail, --json) |

### SSE Follow Mode

The `trigger --follow` command establishes the SSE connection *before* sending the
trigger request. This prevents a race condition where the trigger fires and
completes before the SSE connection is established, which would cause the client
to miss events. The implementation:

1. Open SSE stream at `/api/events?job_id={id}`.
2. Send POST to `/api/jobs/{id}/trigger`.
3. Parse the trigger response to get the `run_id`.
4. Filter SSE events by `run_id`.
5. Print output events to stdout.
6. Exit on `completed` or `failed` event.

The SSE text protocol is parsed manually (line-by-line: `event:`, `data:`, blank
line) rather than using an SSE client library.

### Output Formatting

- Job lists use a pretty-printed table with columns for status, name, schedule,
  last run, and next run.
- Relative time formatting (e.g., "2 hours ago", "in 5 minutes").
- Byte size formatting (B, KB, MB).
- `--json` flag outputs raw JSON for machine consumption.

---

## 12. Web Dashboard

Source: `frontend/` (Next.js with TypeScript and Tailwind CSS)
Build output: `web/` (static export, generated by `build.rs` during `cargo build`)

The frontend is a Next.js application configured for static export (`output: "export"`
in `next.config.ts`). The `build.rs` script runs `npm run build` and copies the
output from `frontend/out/` to `web/`, where `rust-embed` embeds it into the binary.
The original vanilla JS/CSS/HTML frontend is preserved in `web.backup/` for reference.

### Features

- **Job table**: displays all jobs with name, schedule, status badge, last run,
  next run, and action buttons (trigger, logs, edit, delete).
- **Toggle switches**: inline enable/disable switches in the job table.
- **Log viewer**: historical log retrieval via `/api/runs/{run_id}/log` combined
  with live SSE streaming for real-time output. Maximum 2000 log lines with FIFO
  removal of oldest lines.
- **Add/Edit modal**: form for creating or editing jobs, supporting shell commands
  and script files, timezone, working directory, and environment variables.
- **Delete confirmation modal**.
- **Toast notifications**: success/error messages with auto-dismiss animation.
- **Health indicator**: small status dot in the header, polled every 5 seconds.

### Polling and Real-time Updates

- Health: polled every 5 seconds via `GET /health`.
- Job list: polled every 10 seconds as a fallback. Primary updates come from SSE
  `job_changed` events, which trigger an immediate re-fetch.
- SSE: `EventSource` connection to `/api/events` with automatic reconnection
  after 3 seconds on error.

### Security

- XSS protection via `escapeHtml()` function applied to all user-provided content
  before DOM insertion.
- No authentication or authorization (daemon binds to localhost only).

### Theming

- Light theme by default.
- Dark theme via `@media (prefers-color-scheme: dark)`.
- All colors use CSS custom properties for consistency.
- Responsive breakpoints at 768px and 640px.

---

## 13. PTY Abstraction

File: `pty/mod.rs`

### Traits

```rust
pub trait PtySpawner: Send + Sync {
    fn spawn(&self, cmd: CommandBuilder, rows: u16, cols: u16)
        -> Result<Box<dyn PtyProcess>>;
}

pub trait PtyProcess: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    fn kill(&mut self) -> io::Result<()>;
    fn wait(&mut self) -> io::Result<ExitStatus>;
}
```

### Implementations

**NoPtySpawner** (production): Uses plain `std::process::Command` with piped
stdout/stderr and null stdin for process spawning. Reliably handles EOF on all
platforms.

**MockPtySpawner** (testing): Configurable mock with:
- Preset output chunks (returned sequentially from `read()`).
- Configurable exit code.
- Optional spawn error.
- Optional `chunk_delay_ms` for timeout testing.

**Platform-specific ExitStatus construction:** The `exit_status_from_code()`
helper constructs an `ExitStatus` from a raw code using platform-specific APIs:
- Windows: `ExitStatus::from_raw(code as u32)`
- Unix: `ExitStatus::from_raw(code << 8)` (the exit code is in the high byte)

---

## 14. Testing Strategy

### Unit Tests (inline `#[cfg(test)]` modules)

Every source module contains inline tests. Test doubles are defined locally in
each test module rather than in a shared test utilities module. This means
`InMemoryJobStore` and `InMemoryLogStore` are independently implemented in at
least four locations:
- `server/mod.rs` tests
- `daemon/mod.rs` tests
- `daemon/executor.rs` tests
- `daemon/scheduler.rs` tests

While this involves code duplication, each test double is tailored to its module's
needs (e.g., the executor's `InMemoryLogStore` includes a `cleanup_calls` tracker).

**Key unit test coverage:**
- `models/job.rs`: serde roundtrips, validation (empty name, UUID-as-name, invalid
  cron, invalid timezone), `NewJob` default enabled, `JobUpdate` partial
  deserialization, `next_run_at` skip behavior.
- `errors.rs`: all error variants, Display output, From conversions.
- `daemon/events.rs`: all event variant serialization, `Arc<str>` serialization,
  broadcast channel behavior (multi-subscriber, lagged subscriber).
- `daemon/scheduler.rs`: next_run computation (boundaries, timezone, DST), FakeClock
  behavior, scheduler dispatch/skip/wake tests using `tokio::time::pause/advance`.
- `daemon/executor.rs`: output + exit code, non-zero exit = Completed, spawn error
  = Failed, event ordering, multi-chunk output, log writer verification, run status
  updates, timeout enforcement, cleanup calls.
- `daemon/service.rs`: platform name, service name, status serialization, no-panic
  checks.
- `daemon/mod.rs`: PID file acquire/release/stale/idempotent, `is_process_alive`,
  config loading (defaults, from file, nonexistent), data directory creation
  (normal + idempotent), shutdown (marks runs as Killed, releases PID file, handles
  empty case), orphaned log cleanup.
- `server/mod.rs`: 24 integration tests using `tower::ServiceExt::oneshot` --
  covers all routes, status codes, error format, job lifecycle, events broadcast,
  health counts.
- `pty/mod.rs`: mock output, non-zero exit, spawn error, multiple chunks, empty
  output, kill, exit status construction.

### Integration Tests (`tests/` directory)

- **api_tests.rs**: Spawns a real Axum server on a random port and uses `reqwest`
  for actual HTTP requests. 11 tests covering health, CRUD, enable/disable,
  trigger, 404, 409, 400, and SSE connection.
- **cli_tests.rs**: Uses `assert_cmd` to invoke the `acs` binary directly.
  6 tests covering `--version`, `--help`, subcommand help output, and no-argument
  behavior.
- **scheduler_tests.rs**: End-to-end tests using real `FsLogStore` with temp
  directories and `MockPtySpawner`. 3 tests: full create-run-verify-logs cycle,
  log cleanup with `max_log_files_per_job = 2`, and spawn failure recording.

### Test Patterns

- **Fake/Mock PTY**: `MockPtySpawner` is the primary mechanism for testing
  execution without launching real processes.
- **Fake Clock**: `FakeClock` combined with `tokio::time::pause()`/`advance()`
  enables deterministic scheduler testing.
- **InMemoryJobStore/LogStore**: trait-based abstraction enables in-memory test
  doubles.
- **`tempfile::TempDir`**: used for filesystem-dependent tests (PID files, log
  stores, config loading).

---

## 15. Cross-Platform Considerations

### Process Execution

| Aspect          | Windows                                | Unix              |
|-----------------|----------------------------------------|-------------------|
| Shell commands  | `cmd.exe /C {command}`                 | `/bin/sh -c {command}` |
| Script files    | `cmd.exe /C {path}` or `powershell.exe -File {path}` for `.ps1` | `/bin/sh {path}` |
| Process alive   | `OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION)` via FFI | `kill(pid, 0)` |
| PID file        | `create_new(true)` on `OpenOptions`    | Same              |
| ExitStatus      | `from_raw(code as u32)`                | `from_raw(code << 8)` |
| Signal handling | `tokio::signal::ctrl_c()` only         | `ctrl_c() + SIGTERM` |

### Service Registration

| Platform | Mechanism        | Service Name           | Location                                      |
|----------|------------------|------------------------|-----------------------------------------------|
| Windows  | Windows Service  | AgentCronScheduler     | Registry: `HKLM\...\Services\AgentCronScheduler` |
| macOS    | launchd plist    | com.acs.scheduler      | `~/Library/LaunchAgents/com.acs.scheduler.plist` |
| Linux    | systemd user unit| acs                    | `~/.config/systemd/user/acs.service`          |

All service configurations launch with `{exe} start --foreground`. Linux uses
`loginctl enable-linger` for persistence across logouts.

### PTY Behavior

`NoPtySpawner` is used for process spawning on all platforms. Executed commands
get piped stdout/stderr rather than a real terminal. Programs that check
`isatty()` will detect non-TTY mode.

### Config and Data Directories

Resolution order for config:
1. `--config` CLI flag
2. `ACS_CONFIG_DIR` environment variable
3. Platform config dir (`dirs::config_dir()/agent-cron-scheduler/config.json`)
4. `{data_dir}/config.json`
5. Built-in defaults

Resolution order for data directory:
1. `--data-dir` CLI flag / `config.data_dir` field
2. `ACS_DATA_DIR` environment variable
3. `dirs::data_dir()/agent-cron-scheduler`

---

## 16. Dependencies

### Runtime Dependencies

| Crate           | Version | Purpose                                         |
|-----------------|---------|--------------------------------------------------|
| tokio           | 1       | Async runtime (full features)                    |
| axum            | 0.8     | HTTP framework                                   |
| tower           | 0.5     | Middleware framework                             |
| tower-http      | 0.6     | Static file serving, CORS, tracing middleware    |
| reqwest         | 0.12    | HTTP client (CLI commands, json+stream features) |
| clap            | 4.5     | CLI argument parsing (derive)                    |
| croner          | 3       | Cron expression parsing and next-occurrence      |
| portable-pty    | 0.9     | PTY abstraction (used for CommandBuilder type)   |
| serde           | 1       | Serialization framework (derive)                 |
| serde_json      | 1       | JSON serialization                               |
| chrono          | 0.4     | Date/time handling (serde feature)               |
| chrono-tz       | 0.10    | IANA timezone database                           |
| tracing         | 0.1     | Structured logging                               |
| tracing-subscriber | 0.3  | Log output (env-filter feature)                  |
| uuid            | 1       | UUID generation (v7 + serde features)            |
| anyhow          | 1       | Flexible error handling                          |
| thiserror       | 2       | Derive-based error types                         |
| async-trait     | 0.1     | Async trait support                              |
| tokio-stream    | 0.1     | Stream utilities (sync feature for BroadcastStream) |
| futures-util    | 0.3     | Stream combinators (filter_map)                  |
| fs4             | 0.13    | File locking (tokio feature, listed but not visibly used) |
| dirs            | 6       | Platform directory conventions                   |
| windows-service | 0.8     | Windows service registration (Windows only)      |

### Dev Dependencies

| Crate          | Version | Purpose                                     |
|----------------|---------|----------------------------------------------|
| tempfile       | 3       | Temporary directories for tests              |
| assert_cmd     | 2       | CLI binary testing                           |
| predicates     | 3       | Assertion matchers for assert_cmd            |
| tokio-test     | 0.4     | Tokio test utilities                         |
| http-body-util | 0.1     | Body collection for Axum integration tests   |

### Dependency Notes

- **portable-pty** is listed as a runtime dependency and its `CommandBuilder` type
  is used throughout the codebase (including by `NoPtySpawner`, which doesn't
  actually use the PTY functionality). This creates a somewhat misleading dependency
  -- the `CommandBuilder` is used as a common command specification format even
  when no PTY is involved.

- **fs4** is listed with the tokio feature but no usage of its file locking API is
  visible in the source code. The JSON job store uses atomic tmp+rename rather than
  file locking.

- **tower-http** CORS feature is enabled but no CORS middleware is configured in
  `create_router()`. The `fs` and `trace` features are used (ServeDir for static
  files).
