# ACS (Agent Cron Scheduler) - Technical Architecture

## 1. System Overview

ACS is a cross-platform cron scheduling daemon written in Rust. It runs as a long-lived background process that manages scheduled jobs defined by cron expressions, executes them via child processes with piped I/O, and exposes a RESTful HTTP API for job management, log retrieval, and real-time event streaming via Server-Sent Events (SSE).

The system follows a layered architecture:

```
  CLI Client (acs)          HTTP Clients / Frontend
       |                          |
       v                          v
  +------------------------------------+
  |          HTTP Server (Axum)        |
  |   routes, SSE, health, assets     |
  +------------------------------------+
       |              |            |
       v              v            v
  +---------+   +-----------+   +--------+
  |Scheduler|-->| Executor  |   | Event  |
  |         |   |           |   | Bus    |
  +---------+   +-----------+   +--------+
       |              |            |
       v              v            v
  +------------------------------------+
  |        Storage Layer (Traits)      |
  |     JobStore       LogStore        |
  +------------------------------------+
       |                   |
       v                   v
  jobs.json         logs/<job_id>/*.log
```

### High-Level Architecture

- **Single-binary deployment**: The `acs` binary serves as both the CLI client and the daemon server. The `main()` function parses CLI arguments and dispatches to the appropriate handler.
- **Async runtime**: Built on Tokio, with the runtime created explicitly in `main()` via `tokio::runtime::Runtime::new()`.
- **Trait-based storage**: All persistence is behind `JobStore` and `LogStore` traits, with concrete implementations using JSON files and filesystem logs.
- **Event-driven**: A broadcast channel propagates `JobEvent` variants to all subscribers (SSE clients, metadata updater, etc.).

---

## 2. Module Structure

### Source Tree

```
acs/src/
  main.rs                     # Entry point: CLI parse + Tokio runtime
  lib.rs                      # Module declarations
  errors.rs                   # AcsError enum (thiserror)
  cli/
    mod.rs                    # Cli struct, Commands enum, dispatch()
    daemon.rs                 # start/stop/status/restart/uninstall handlers
    jobs.rs                   # add/remove/list/enable/disable/trigger handlers
    logs.rs                   # logs command handler
  daemon/
    mod.rs                    # PidFile, PortFile, load_config(), start_daemon(),
                              #   graceful_shutdown(), SizeManagedWriter,
                              #   resolve_data_dir(), create_data_dirs(),
                              #   cleanup_orphaned_logs()
    scheduler.rs              # Scheduler, Clock trait, SystemClock, FakeClock,
                              #   compute_next_run()
    executor.rs               # Executor, RunHandle
    events.rs                 # JobEvent enum, JobChangeKind enum
    service.rs                # OS service registration (Windows/macOS/Linux)
  server/
    mod.rs                    # AppState, create_router()
    routes.rs                 # REST API route handlers
    sse.rs                    # SSE event streaming endpoint
    health.rs                 # GET /health handler
    assets.rs                 # Embedded static file serving (SPA fallback)
  storage/
    mod.rs                    # JobStore trait, LogStore trait
    jobs.rs                   # JsonJobStore (JSON file persistence)
    logs.rs                   # FsLogStore (filesystem log storage)
  models/
    mod.rs                    # Re-exports
    job.rs                    # Job, NewJob, JobUpdate, ExecutionType,
                              #   validate_new_job(), validate_job_update()
    run.rs                    # JobRun, RunStatus
    config.rs                 # DaemonConfig
  pty/
    mod.rs                    # PtySpawner trait, PtyProcess trait,
                              #   NoPtySpawner, MockPtySpawner
```

### Module Responsibilities

#### `cli` -- Command-Line Interface

- **`cli::Cli`**: Top-level clap `Parser` struct with global options.
- **`cli::Commands`**: Enum of all subcommands.
- **`cli::dispatch()`**: Routes parsed CLI commands to handler functions. Most commands communicate over HTTP to the daemon's REST API; `Start` either runs the daemon directly or spawns it.

See [CLI Reference](cli-reference.md) for the full command documentation.

#### `daemon` -- Daemon Lifecycle and Core Engine

- **`daemon::start_daemon()`**: The master orchestration function. Acquires PID file, loads config, creates data directories, initializes storage, sets up channels, starts the Scheduler, Executor dispatch loop, metadata updater, and HTTP server, then waits for shutdown signals.
- **`daemon::PidFile`**: Manages an exclusive PID file (`acs.pid`) to enforce single-instance. Uses `create_new(true)` for atomic creation with stale PID detection.
- **`daemon::PortFile`**: Writes the actual bound port to `acs.port` so CLI clients can discover the daemon.
- **`daemon::load_config()`**: Loads `DaemonConfig` via a multi-level resolution order (see [Configuration](configuration.md)).
- **`daemon::resolve_data_dir()`**: Resolves the data directory from CLI override, env var, or platform default (see [Configuration](configuration.md#data-directory-locations)).
- **`daemon::graceful_shutdown()`**: Implements the shutdown sequence (see Section 3.4).
- **`daemon::cleanup_orphaned_logs()`**: Removes log directories for deleted jobs on startup (see [Storage](storage.md#6-orphaned-log-cleanup)).

#### `daemon::scheduler` -- Cron Scheduling Engine

- **`Scheduler`**: Long-lived async task that polls enabled jobs from the `JobStore`, computes next run times using `compute_next_run()`, sleeps until the earliest due time, and dispatches due jobs over an `mpsc` channel.
- **`Clock` trait**: Abstracts system time. Implementations: `SystemClock` (production), `FakeClock` (testing with controllable time).
- **`compute_next_run()`**: Evaluates a cron expression using the `croner` crate. Supports optional IANA timezone via `chrono-tz` -- converts to local time, finds next occurrence, then converts back to UTC.

#### `daemon::executor` -- Job Execution Engine

- **`Executor`**: Spawns child processes for jobs. Each `spawn_job()` call creates a `JobRun` record, broadcasts a `Started` event, spawns the process via the `PtySpawner` trait, and manages the output/log pipeline.
- **`RunHandle`**: Returned by `spawn_job()`. Contains `run_id`, `job_id`, `join_handle` (the Tokio task handle), and `kill_tx` (a oneshot channel to signal cancellation).
- **`Executor::build_command()`**: Constructs a `portable_pty::CommandBuilder` from the job's `ExecutionType` (see [Job Management](job-management.md#execution-types) for platform-specific shell behavior).

#### `daemon::events` -- Event System

- **`JobEvent`**: Tagged enum with variants `Started`, `Output`, `Completed`, `Failed`, `JobChanged`. Each variant carries `job_id`, `run_id` (where applicable), a `timestamp`, and variant-specific data.
- **`JobChangeKind`**: Enum with variants `Added`, `Updated`, `Removed`, `Enabled`, `Disabled`.
- Events are serialized as JSON with `#[serde(tag = "event", content = "data")]` for SSE streaming.
- `Output` data uses `Arc<str>` for zero-copy cloning across broadcast subscribers.

#### `server` -- HTTP Server

- **`AppState`**: Central shared state struct holding `job_store`, `log_store`, `event_tx`, `scheduler_notify`, `config`, `start_time`, `active_runs`, `shutdown_tx`, and `dispatch_tx`.
- **`create_router()`**: Builds the Axum `Router` with all API routes, CORS middleware (permissive), and a fallback to embedded static assets.
- Routes cover job CRUD, run/log retrieval, SSE streaming, health, shutdown, restart, and daemon logs. See [API Reference](api-reference.md) for the full endpoint specification.
- Error responses use consistent `{ "error": "...", "message": "..." }` JSON format.

#### `storage` -- Persistence Layer

- **`JobStore` trait**: Async trait with methods `list_jobs`, `get_job`, `find_by_name`, `create_job`, `update_job`, `delete_job`.
- **`LogStore` trait**: Async trait with methods `create_run`, `update_run`, `append_log`, `read_log`, `list_runs`, `cleanup`.
- **`JsonJobStore`**: Concrete `JobStore` using JSON file persistence with in-memory cache.
- **`FsLogStore`**: Concrete `LogStore` using filesystem-based per-job log directories.

See [Storage](storage.md) for implementation details.

#### `models` -- Data Types

- **`Job`**: Core job struct with identity, scheduling, execution config, and lifecycle metadata. See [Job Management](job-management.md) for the full field reference.
- **`NewJob`**: Input struct for job creation. **`JobUpdate`**: Partial update struct with all optional fields.
- **`ExecutionType`**: Tagged enum: `ShellCommand(String)` or `ScriptFile(String)`.
- **`JobRun`**: Run record. **`RunStatus`**: Enum with `Running`, `Completed`, `Failed`, `Killed`.
- **`DaemonConfig`**: Configuration struct with serde defaults. See [Configuration](configuration.md) for the full field reference.

#### `pty` -- Process Spawning Abstraction

- **`PtySpawner` trait**: `fn spawn(&self, cmd: CommandBuilder, rows: u16, cols: u16) -> Result<Box<dyn PtyProcess>>`.
- **`PtyProcess` trait**: `fn read()`, `fn kill()`, `fn wait()` for managing spawned processes.
- **`NoPtySpawner`**: Production implementation using `std::process::Command` with piped stdout/stderr.
- **`MockPtySpawner`**: Test double with configurable output and exit codes.

#### `errors` -- Error Types

- **`AcsError`**: `thiserror`-based enum with variants: `NotFound`, `Conflict`, `Validation`, `Storage`, `Internal`, `Cron`, `Pty`, `Timeout`.
- Implements `From<std::io::Error>`, `From<serde_json::Error>`, `From<uuid::Error>` for ergonomic error conversion.

---

## 3. Data Flow

### 3.1 Startup Sequence

The `start_daemon()` function in `daemon::mod.rs` orchestrates startup:

```
1.  load_config()           -- Load DaemonConfig (5-level resolution)
2.  Apply CLI overrides     -- host_override, port_override
3.  resolve_data_dir()      -- Determine data directory
4.  create_data_dirs()      -- Ensure data/, data/logs/, data/scripts/ exist
5.  Set up tracing          -- stderr + daemon.log via SizeManagedWriter
6.  PidFile::acquire()      -- Exclusive PID file (acs.pid)
7.  JsonJobStore::new()     -- Load jobs.json into memory cache
8.  FsLogStore::new()       -- Initialize logs directory
9.  cleanup_orphaned_logs() -- Remove log dirs for deleted jobs
10. broadcast::channel()    -- Create event bus (capacity from config)
11. Notify::new()           -- Create scheduler wake signal
12. watch::channel()        -- Create shutdown signal
13. mpsc::channel(64)       -- Create dispatch channel (scheduler -> executor)
14. Build AppState           -- Aggregate all shared state
15. Executor::new()          -- Create executor with NoPtySpawner
16. Scheduler::new()         -- Create scheduler
17. tokio::spawn(scheduler)  -- Start scheduler loop
18. tokio::spawn(dispatch)   -- Start dispatch loop (recv jobs, call executor)
19. tokio::spawn(updater)    -- Start metadata updater (listen for events)
20. TcpListener::bind()      -- Bind HTTP server
21. PortFile::write_to()     -- Write actual port to acs.port
22. tokio::spawn(server)     -- Start Axum server with graceful shutdown
23. Wait for signal          -- Ctrl+C, SIGTERM (Unix), or API shutdown
```

### 3.2 Job Scheduling Flow

```
                  Scheduler::run() loop
                         |
            1. job_store.list_jobs()
                         |
            2. Filter enabled jobs
                         |
            3. compute_next_run() for each
                         |
            4. Find earliest next_time
                         |
     +-------------------+--------------------+
     |                                        |
  tokio::time::sleep(duration)         notify.notified()
     |                                        |
  5. Re-check clock, dispatch         Re-loop from step 1
     due jobs via dispatch_tx.send()
     |
     v
  Dispatch loop receives Job
     |
  executor.spawn_job(&job)
     |
  RunHandle stored in active_runs
```

When the job list changes (create/update/delete via API), the route handler calls `scheduler_notify.notify_one()` to wake the scheduler, causing it to re-evaluate all enabled jobs from the top.

### 3.3 Job Execution Flow

```
executor.spawn_job(&job)
    |
    1. Generate run_id (UUIDv7)
    2. Create JobRun {status: Running} in log_store
    3. Broadcast JobEvent::Started
    4. build_command() -> CommandBuilder
    5. Create oneshot kill channel (kill_tx, kill_rx)
    |
    tokio::spawn(async move {
        |
        6. pty_spawner.spawn(cmd, rows, cols)
        |   (NoPtySpawner: std::process::Command with piped I/O)
        |
        7. Optionally dump environment (if log_environment)
        8. Write command header ("$ <cmd>\n") to log
        |
        9. Create mpsc::channel(256) for log writer
        10. Spawn log writer task (async: recv bytes, append to log_store)
        11. Spawn blocking PTY read loop (spawn_blocking)
        |    reads 8192-byte chunks, sends via mpsc to async side
        |
        12. Output forwarding loop:
            tokio::select! {
                chunk from output_rx  -> broadcast Output event + send to log writer
                kill_rx               -> set killed=true, break
                timeout_fut           -> set timed_out=true, break
            }
        |
        13. Drop log_tx (signals log writer to finish)
        14. Await read_handle (get exit status)
        15. Await log_writer_handle (get total_bytes)
        |
        16. Determine outcome:
            - timed_out  -> update run to Failed, broadcast Failed
            - killed     -> update run to Killed, broadcast Failed
            - Ok(status) -> update run to Completed, broadcast Completed
            - Err(e)     -> update run to Failed, broadcast Failed
        |
        17. log_store.cleanup(job_id, max_log_files)
    })
    |
    Return RunHandle { run_id, job_id, join_handle, kill_tx }
```

A separate **metadata updater** task subscribes to the broadcast channel and updates job-level metadata (`last_run_at`, `last_exit_code`) on `Completed` and `Failed` events by calling `job_store.update_job()`.

### 3.4 Shutdown Sequence

Triggered by Ctrl+C, SIGTERM (Unix), or `POST /api/shutdown`:

```
1. Send () on watch::Sender      -- Signals HTTP server to stop accepting connections
2. scheduler_handle.abort()       -- Stop scheduling new runs
3. dispatch_handle.abort()        -- Stop dispatching new runs
4. updater_handle.abort()         -- Stop metadata updater
5. graceful_shutdown():
   a. Lock active_runs (write)
   b. For each active RunHandle:
      - Send () on kill_tx         -- Signal task to stop
      - Await join_handle with 30s timeout
   c. For each in-flight run:
      - Update JobRun to Killed status with finished_at and error message
   d. PidFile::release()           -- Remove acs.pid
   e. PortFile::remove()           -- Remove acs.port
6. Await server_handle             -- Wait for HTTP server to finish
7. Exit with code 0
```

---

## 4. Concurrency Model

ACS uses several Tokio async primitives to coordinate between components:

### 4.1 Broadcast Channel -- Event Bus

```rust
let (event_tx, _event_rx) = broadcast::channel::<JobEvent>(config.broadcast_capacity);
```

- **Purpose**: Fan-out of `JobEvent` variants to multiple subscribers.
- **Capacity**: Configurable via `DaemonConfig::broadcast_capacity` (default 4096).
- **Producers**: `Executor` (Started, Output, Completed, Failed), API route handlers (JobChanged).
- **Consumers**: SSE handler (streams to HTTP clients), metadata updater task, any new subscriber via `event_tx.subscribe()`.
- **Backpressure**: Slow consumers receive `RecvError::Lagged(n)` and skip missed events.
- **Clone semantics**: `JobEvent::Output` uses `Arc<str>` for the data payload, making broadcast clones cheap (pointer copy, not data copy).

### 4.2 MPSC Channel -- Job Dispatch

```rust
let (dispatch_tx, dispatch_rx) = tokio::sync::mpsc::channel::<Job>(64);
```

- **Purpose**: Delivers due jobs from the Scheduler to the dispatch loop, which calls `Executor::spawn_job()`.
- **Capacity**: 64 pending jobs.
- **Producers**: `Scheduler::run()` sends due jobs; API trigger endpoint sends manually-triggered jobs via a cloned `dispatch_tx`.
- **Consumer**: Single dispatch loop task that calls `executor.spawn_job()` and stores the resulting `RunHandle` in `active_runs`.

### 4.3 Notify -- Scheduler Wake

```rust
let scheduler_notify = Arc::new(Notify::new());
```

- **Purpose**: Wakes the Scheduler when the job list changes so it can re-evaluate schedules.
- **Producers**: API route handlers call `scheduler_notify.notify_one()` after create/update/delete/enable/disable operations.
- **Consumer**: `Scheduler::run()` uses `tokio::select!` between `tokio::time::sleep(duration)` and `notify.notified()`.

### 4.4 Watch Channel -- Shutdown Signal

```rust
let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(());
```

- **Purpose**: Broadcasts a shutdown signal to the HTTP server's graceful shutdown handler.
- **Producers**: `start_daemon()` sends `()` after receiving Ctrl+C, SIGTERM, or API shutdown.
- **Consumer**: Axum's `with_graceful_shutdown()` awaits `shutdown_rx.changed()`. Also used by the main loop via `shutdown_tx.subscribe()` to detect API-initiated shutdown.

### 4.5 Oneshot Channel -- Per-Run Kill Signal

```rust
let (kill_tx, kill_rx) = tokio::sync::oneshot::channel::<()>();
```

- **Purpose**: Allows cancellation of a specific running job.
- **One per run**: Created inside `Executor::spawn_job()`, with `kill_tx` stored in the `RunHandle`.
- **Producer**: `graceful_shutdown()` sends `()` to kill all active runs; could also be used by a future per-job kill API.
- **Consumer**: The execution task's `tokio::select!` loop breaks on `kill_rx`, setting `killed = true`.

### 4.6 RwLock -- Shared State Protection

```rust
// Job store cache
cache: RwLock<Vec<Job>>              // inside JsonJobStore

// Active runs tracking
active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>>  // in AppState
```

- **`JsonJobStore::cache`**: Tokio `RwLock<Vec<Job>>`. Read lock for `list_jobs`, `get_job`, `find_by_name`. Write lock for `create_job`, `update_job`, `delete_job` (each followed by `persist()` to disk).
- **`active_runs`**: Tokio `RwLock<HashMap<Uuid, RunHandle>>`. Write lock when inserting new handles (dispatch loop) or draining during shutdown. Read lock potentially for status queries.

### 4.7 Arc Sharing

All major components are shared via `Arc`:

| Resource | Type | Shared Between |
|---|---|---|
| `job_store` | `Arc<dyn JobStore>` | AppState, Scheduler, metadata updater |
| `log_store` | `Arc<dyn LogStore>` | AppState, Executor, graceful_shutdown |
| `config` | `Arc<DaemonConfig>` | AppState, Executor |
| `scheduler_notify` | `Arc<Notify>` | AppState, Scheduler |
| `active_runs` | `Arc<RwLock<HashMap<Uuid, RunHandle>>>` | AppState, dispatch loop, graceful_shutdown |
| `event_tx` | `broadcast::Sender<JobEvent>` | AppState, Executor, Scheduler, metadata updater |
| `pty_spawner` | `Arc<dyn PtySpawner>` | Executor |

### 4.8 Blocking Work

PTY/process output reading is performed in `tokio::task::spawn_blocking()` because `std::process::ChildStdout::read()` is a blocking call. The blocking task sends output chunks to the async side via an `mpsc::channel(256)`.

---

## 5. Key Design Decisions

### 5.1 Trait-Based Storage

Storage interfaces are async traits (`JobStore`, `LogStore`) behind `Arc<dyn ...>`. This decouples business logic from persistence, enables in-memory test doubles without touching the filesystem, and leaves open the possibility of future storage backends (e.g., SQLite).

### 5.2 PID File Locking

Single-instance enforcement uses `create_new(true)` (maps to `O_EXCL`/`CREATE_NEW`) for atomic filesystem locking. Stale PID files are detected by checking process liveness (`kill(pid, 0)` on Unix, `OpenProcess` on Windows). Restart overlap is tolerated via 10-second retry loop.

### 5.3 Piped I/O over PTY

The production `NoPtySpawner` uses `std::process::Command` with piped stdout/stderr rather than a real PTY. Piped I/O reliably delivers EOF on all platforms, avoiding platform-specific PTY issues. On Windows, `raw_arg()` bypasses Rust's MSVC quoting for `cmd.exe` compatibility.

### 5.4 Atomic File Persistence

`JsonJobStore` uses write-to-temp-then-rename for crash safety. Corrupted `jobs.json` is backed up to `.bak` and the store starts empty. The same pattern is used for `daemon.log` truncation.

### 5.5 Event-Driven Architecture

The broadcast channel enables fan-out of `JobEvent` variants to SSE clients, the metadata updater, and any future subscriber. `Arc<str>` in output events makes broadcast cloning cheap. Slow subscribers receive `Lagged` errors rather than blocking producers.

### 5.6 Timezone-Aware Scheduling

Cron expressions are evaluated in the job's IANA timezone via `chrono-tz`: convert UTC to local, find next cron occurrence, convert back to UTC. DST transitions are handled by the `croner` crate.

### 5.7 UUIDv7 Identifiers

All IDs use `Uuid::now_v7()` for natural time-ordering, uniqueness without coordination, and monotonically increasing values.
