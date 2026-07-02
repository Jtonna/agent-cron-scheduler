# ACS (Agent Cron Scheduler) - Technical Architecture

## 1. System Overview

ACS is a cross-platform cron scheduling daemon written in Rust. It runs as a long-lived background process that manages scheduled **workflows** defined by cron expressions, executes their steps via child processes with piped I/O, and exposes a RESTful HTTP API for workflow management, run retrieval, and real-time event streaming via Server-Sent Events (SSE).

The system follows a layered architecture:

```
  CLI Client (acs)          HTTP Clients / Frontend
       |                          |
       v                          v
  +------------------------------------+
  |          HTTP Server (Axum)        |
  |   workflow routes, SSE, health,    |
  |   assets                           |
  +------------------------------------+
       |              |            |
       v              v            v
  +----------+  +----------+   +--------+
  |Workflow  |->| Workflow |   | Event  |
  |Scheduler |  | Executor |   | Bus    |
  +----------+  +----------+   +--------+
       |              |            |
       v              v            v
  +------------------------------------+
  |        Storage Layer (Traits)      |
  |  WorkflowStore   WorkflowRunStore  |
  +------------------------------------+
       |                   |
       v                   v
  acs.db (SQLite: workflows, workflow_runs)
                    logs/<workflow_id>/<run_id>.log
```

### High-Level Architecture

- **Single-binary deployment**: The `acs` binary serves as both the CLI client and the daemon server. `main()` parses CLI arguments and dispatches to the appropriate handler (`acs/src/main.rs`).
- **Async runtime**: Built on Tokio, with the runtime created explicitly in `main()` via `tokio::runtime::Runtime::new()`.
- **Trait-based storage**: All persistence is behind `WorkflowStore` and `WorkflowRunStore` traits, with concrete SQLite-backed implementations.
- **Event-driven**: A broadcast channel propagates `WorkflowEvent` variants to all subscribers (SSE clients, etc.).
- **Workflow-native runtime**: The entire execution model is built around `Workflow` and `StepDef`. There is no separate `Job` concept.

---

## 2. Module Structure

### Source Tree

```
acs/src/
  main.rs                          # Entry point: CLI parse + Tokio runtime
  lib.rs                           # Module declarations
  errors.rs                        # AcsError enum (thiserror)
  cli/
    mod.rs                         # Cli struct, Commands enum, dispatch()
    daemon.rs                      # start/stop/status/restart/uninstall handlers
    workflows.rs                   # workflow CRUD + trigger + runs subcommands
  daemon/
    mod.rs                         # PidFile, PortFile, load_config(), start_daemon(),
                                   #   graceful_shutdown(), SizeManagedWriter,
                                   #   resolve_data_dir(), create_data_dirs()
    scheduler.rs                   # WorkflowScheduler, Clock trait, SystemClock,
                                   #   FakeClock, compute_next_run()
    events.rs                      # WorkflowEvent enum, WorkflowChangeKind enum
    service.rs                     # OS service registration (Windows/macOS/Linux)
    cost_cache.rs                  # CostCache — per-workflow CostSummary + 365-day daily-bucket
                                   #   cache; system aggregate; broadcast-driven eager
                                   #   invalidation. Fronts the cost_summary_for /
                                   #   daily_buckets_for store methods.
  server/
    mod.rs                         # AppState, create_router()
    workflow_routes.rs             # REST API route handlers for workflows and runs
    routes.rs                      # Misc routes: shutdown, restart, daemon logs
    sse.rs                         # GET /api/events/workflows SSE handler
    health.rs                      # GET /health handler
    assets.rs                      # Embedded static file serving (SPA fallback)
  storage/
    mod.rs                         # Module declarations: sqlite, workflow_runs, workflows
    workflows.rs                   # WorkflowStore trait
    workflow_runs.rs               # WorkflowRunStore trait
    sqlite/
      mod.rs                       # SqliteDb handle, init_db(), init_in_memory_db()
      schema.rs                    # SCHEMA_SQL, apply_pragmas(), apply_schema()
      row_helpers.rs               # parse_dt, parse_opt_dt, run_status_str, error mappers
      workflows.rs                 # SqliteWorkflowStore (impls WorkflowStore)
      workflow_runs.rs             # SqliteWorkflowRunStore (impls WorkflowRunStore)
  models/
    mod.rs                         # Re-exports
    workflow.rs                    # Workflow, StepDef enum + variants, StepDefCommon,
                                   #   CaptureSpec, FailurePolicy, RunStatus,
                                   #   WorkflowRun, StepRun, TriggerParams,
                                   #   NewWorkflow, WorkflowUpdate, AgentType
    config.rs                      # DaemonConfig
  workflow/
    mod.rs                         # Re-exports: run_workflow, finalize_run, FileLogSink,
                                   #   EventEmittingLogSink, Step, StepContext,
                                   #   StepOutput, StepError, CostFragment, LogSink
    step.rs                        # Step trait, StepContext, StepOutput, StepError,
                                   #   CostFragment, LogSink trait, KillSender/Receiver,
                                   #   wait_for_kill()
    executor.rs                    # run_workflow() — the step loop entry point;
                                   #   MatchStep is handled inline in execute_steps()
                                   #   rather than as a separate Step impl
    finalize.rs                    # finalize_run() — shared post-run plumbing called by
                                   #   both the scheduler dispatch path and the
                                   #   /api/workflows/{id}/trigger route to persist the
                                   #   terminal WorkflowRun and stamp last_run_* on the
                                   #   parent workflow
    template.rs                    # substitute() — ${input.*} and ${steps.*.*}
    log_sink.rs                    # FileLogSink (concrete LogSink for combined run log)
    event_log_sink.rs              # EventEmittingLogSink (LogSink wrapper for SSE chunks)
    steps/
      mod.rs                       # Sub-module declarations
      shell.rs                     # ShellStep impl
      script.rs                    # ScriptStep impl
      http.rs                      # HttpStep impl
      set_var.rs                   # SetVarStep impl
      agent.rs                     # AgentStep impl
    agents/
      mod.rs                       # AgentImpl trait, AgentOutputParser trait,
                                   #   AgentOutput, impl_for()
      claude_code_cli.rs           # ClaudeCodeCli impl + ClaudeStreamParser
  process_kill.rs                  # kill_process_tree(), force_kill_process_tree()
  pty/
    mod.rs                         # PtySpawner trait, PtyProcess trait,
                                   #   NoPtySpawner, MockPtySpawner
```

### Module Responsibilities

#### `cli` -- Command-Line Interface

- **`cli::Cli`**: Top-level clap `Parser` struct with global options.
- **`cli::Commands`**: Enum of all subcommands (`Start`, `Stop`, `Restart`, `Status`, `Uninstall`, `Update`, `Workflows`).
- **`cli::dispatch()`**: Routes parsed CLI commands to handler functions. Most commands communicate over HTTP to the daemon's REST API; `Start` either runs the daemon directly or spawns it.
- **`cli::workflows`**: Subcommands for workflow CRUD, trigger, and run listing. Communicates with the daemon over the REST API.

See [CLI Reference](cli-reference.md) for the full command documentation.

#### `daemon` -- Daemon Lifecycle and Core Engine

- **`daemon::start_daemon()`**: The master orchestration function. Acquires PID file, loads config, creates data directories, runs pending migrations, opens `acs.db` and initializes storage (`SqliteWorkflowStore`, `SqliteWorkflowRunStore`), sets up the broadcast channel, starts the `WorkflowScheduler`, and starts the HTTP server, then waits for shutdown signals.
- **`daemon::PidFile`**: Manages an exclusive PID file (`agentcronsystem.pid`) to enforce single-instance. Uses `create_new(true)` for atomic creation with stale PID detection.
- **`daemon::PortFile`**: Writes the actual bound port to `agentcronsystem.port` so CLI clients can discover the daemon.
- **`daemon::load_config()`**: Loads `DaemonConfig` via a multi-level resolution order (see [Configuration](configuration.md)).
- **`daemon::resolve_data_dir()`**: Resolves the data directory from CLI override, env var, or platform default (see [Configuration](configuration.md#data-directory-locations)).
- **`daemon::graceful_shutdown()`**: Removes PID and port files on daemon exit.

#### `daemon::scheduler` -- Cron Scheduling Engine

- **`WorkflowScheduler`**: Long-lived async task that polls enabled workflows from the `WorkflowStore`, computes next run times using `compute_next_run()`, sleeps until the earliest due time, and dispatches due workflows by calling `run_workflow()` directly (no intermediate dispatch channel). Each dispatch spawns a Tokio task, creates a `FileLogSink` wrapped in `EventEmittingLogSink`, persists an initial `Running` run record, and awaits the final `WorkflowRun` result for persistence.
- **`Clock` trait**: Abstracts system time. Implementations: `SystemClock` (production), `FakeClock` (testing with controllable time).
- **`compute_next_run()`**: Evaluates a cron expression using the `croner` crate. Supports optional IANA timezone via `chrono-tz` — converts to local time, finds next occurrence, then converts back to UTC.

#### `daemon::events` -- Event System

- **`WorkflowEvent`**: Tagged enum (`#[serde(tag = "type")]`) with variants `RunStarted`, `StepStarted`, `StepOutput`, `StepCompleted`, `RunCompleted`, `RunFailed`, `WorkflowChanged`. Each variant carries `run_id`, `workflow_id`, step-level fields where applicable, and timestamps.
- **`WorkflowChangeKind`**: Enum with variants `Created`, `Updated`, `Deleted`, `Enabled`, `Disabled`.
- `StepOutput.data` uses `Arc<str>` for zero-copy cloning across broadcast subscribers.

#### `server` -- HTTP Server

- **`AppState`**: Central shared state holding `workflow_store`, `workflow_run_store`, `workflow_event_tx`, `scheduler_notify`, `config`, `start_time`, `kill_signals`, and `shutdown_tx`.
- **`create_router()`**: Builds the Axum `Router` with all API routes, permissive CORS middleware, and a fallback to embedded static assets. Routes: workflow CRUD (`/api/workflows`, `/api/workflows/{id}`), trigger (`/api/workflows/{id}/trigger`), runs list (`/api/workflows/{id}/runs`), recent runs feed (`GET /api/runs/recent` — cross-workflow, paginated), run detail (`/api/runs/{run_id}`), run log (`GET /api/runs/{id}/log` — byte-range log slicing with `step_index`, `from`, `to` query params), kill (`/api/runs/{run_id}/kill`), cost analytics (`/api/cost/workflows`, `/api/cost/workflows/{id}`), SSE (`/api/events/workflows`), health (`/health`), shutdown (`/api/shutdown`), restart (`/api/restart`), daemon logs (`/api/logs`).
- See [API Reference](api-reference.md) for the full endpoint specification.

#### `storage` -- Persistence Layer

- **`WorkflowStore` trait**: Async trait with `list_workflows`, `get_workflow`, `find_by_name`, `create_workflow`, `update_workflow`, `delete_workflow`, `record_run_outcome`. (`acs/src/storage/workflows.rs`)
- **`SqliteWorkflowStore`**: Concrete `WorkflowStore` backed by the `workflows` table in `<data_dir>/acs.db`. Holds a shared `SqliteDb` (`Arc<Mutex<rusqlite::Connection>>`) and offloads every call to `tokio::task::spawn_blocking` so the runtime is never blocked by synchronous DB calls.
- **`WorkflowRunStore` trait**: Async trait with `create_run`, `update_run`, `get_run`, `list_runs(workflow_id, limit, offset)`, `count_runs`, `delete_run`, `list_recent_runs`, `count_all_runs`, `cost_summary_for` (the entry point used by `CostCache`), `daily_buckets_for`. (`acs/src/storage/workflow_runs.rs`)
- **`SqliteWorkflowRunStore`**: Concrete `WorkflowRunStore` backed by the `workflow_runs` table in `<data_dir>/acs.db`. Shares the same `SqliteDb` handle as `SqliteWorkflowStore`. `list_runs` orders by `run_id DESC`, which is latest-first because `run_id` is a UUIDv7.

**Cost analytics caching.** Workflow cost summaries (30-day + 1-year totals) are computed on demand by `acs::storage::sqlite::SqliteWorkflowRunStore::cost_summary_for(workflow_id, display_tz)` and cached in memory by `acs::daemon::cost_cache::CostCache`, held in the shared daemon state alongside the workflow/run stores. The cache subscribes to the internal `tokio::sync::broadcast::Sender<WorkflowEvent>` via a background task; when `RunCompleted` or `RunFailed` fires (covering Completed/Failed/Killed terminal statuses), the affected workflow's cache entry is eagerly recomputed. Each cached entry also carries a `valid_until` timestamp set to the next calendar-day midnight in `display_timezone`, after which the next read triggers a recompute. Workflow deletion evicts the entry via `CostCache::forget`. The cache is consulted exclusively by the dedicated cost handlers at `GET /api/cost/workflows` and `GET /api/cost/workflows/{id}` — the lean workflow endpoints (`GET /api/workflows[/{id}]`) do not read the cost cache.

**Daily cost buckets (v4.2.9).** In addition to the rolled-up 30-day and 1-year scalars, the cache also holds a 365-day array of `DailyBucket` entries per workflow plus a system-wide 365-day array. Each entry carries the day's local date (in `display_timezone`), total cost, and status-broken counts + cost (`cost_from_completed`, `cost_from_failed`, `cost_from_killed`, `runs_completed`, `runs_failed`, `runs_killed`). Sub-window requests slice the cached array in memory. The eager invalidator on `RunCompleted`/`RunFailed` refreshes both the affected workflow's 365-day array AND the system 365-day array. Window slices are served from `GET /api/cost/workflows[/{id}]`.

See [Storage](storage.md) for implementation details.

#### `models` -- Data Types

- **`Workflow`**: Core struct with identity, scheduling, steps, and lifecycle metadata. `next_run_at` is computed at runtime and never persisted (`#[serde(skip_deserializing)]`).
- **`NewWorkflow`** / **`WorkflowUpdate`**: Input structs for creation and partial update.
- **`StepDef`**: Tagged enum (`#[serde(tag = "kind", rename_all = "snake_case")]`) with variants `Shell`, `Script`, `Http`, `Match`, `SetVar`, `Agent`. Each variant embeds `StepDefCommon` via `#[serde(flatten)]`.
- **`StepDefCommon`**: Shared fields: `id`, `on_failure`, `always_run`, `timeout_secs`, `working_dir`, `env_vars`, `capture`.
- **`ScheduleMode`**: `Cron` (default — always dispatch when due) or `WaitForCompletion` (skip dispatch if a run of this workflow is already active).
- **`TriggerParams`**: Per-invocation overrides: `input` (replaces `workflow.default_input` for one run), `env` (merges onto `workflow.env_vars`), `target_step` (optional step routing).
- **`WorkflowRun`** / **`StepRun`**: Run record types. `WorkflowRun.workflow_snapshot` is a full copy of the `Workflow` definition at trigger time.
- **`RunStatus`**: `Running | Completed | Failed | Killed`.
- **`FailurePolicy`**: `Abort | Continue | Retry { attempts, backoff_ms }`.
- **`AgentType`**: `ClaudeCodeCli` (extensible via new variants).
- **`DaemonConfig`**: Configuration struct with serde defaults. See [Configuration](configuration.md).

#### `workflow` -- Workflow Runtime

- **`run_workflow()`** (`acs/src/workflow/executor.rs`): Public entry point. Takes a `&Workflow`, a pre-generated `run_id`, `TriggerParams`, an `Arc<dyn LogSink>`, an optional `broadcast::Sender<WorkflowEvent>`, and an optional kill-signals registry. Returns a fully-populated `WorkflowRun`. Emits `RunStarted` before execution; emits `RunCompleted` for `Completed` status, `RunFailed` for both `Failed` AND `Killed` status.
- **`execute_steps()`**: Internal recursive function. Walks steps in order, evaluating `always_run` / `aborted` / `killed` flags. Handles `MatchStep` inline by evaluating the expression template and recursing into the chosen branch. Emits `StepStarted` and `StepCompleted` events per step.
- **`run_step_with_policy()`**: Wraps `dispatch_step()` with retry logic. Retry exhaustion is treated as `Abort`.
- **`Step` trait** (`acs/src/workflow/step.rs`): `fn kind() -> &'static str; async fn execute(ctx: &mut StepContext) -> Result<StepOutput, StepError>`. Implemented by each step kind.
- **`StepContext`**: Mutable execution context passed to each step. Carries `input`, `steps: IndexMap<String, StepOutput>` (accumulated step outputs keyed by step id; insertion-ordered so `pass_stdin` selects the immediately-prior step deterministically), `log_sink`, `env`, `working_dir`, `event_tx`, and `kill_rx`.
- **`LogSink` trait**: `write_step_start`, `write_chunk`, `write_step_end`, plus a defaulted `set_current_step`. Implemented by `FileLogSink` and wrapped by `EventEmittingLogSink`.
- **`template::substitute()`** (`acs/src/workflow/template.rs`): Single-pass `${...}` substitution. Namespaces: `input.<dotted.path>` and `steps.<step_id>.(stdout|exit_code|exports.<name>)`. Within a known namespace, missing references resolve to empty string with a logged warning. Tokens whose top-level segment is neither `input.` nor `steps.` (e.g. `${prompt}` consumed by the agent step's second pass) are left intact in the output so layered substitution passes can handle them.

#### `workflow::steps` -- Step Implementations

- **`ShellStep`**: Spawns `/bin/sh -c <command>` (Unix) or `cmd.exe /C <command>` (Windows) via `NoPtySpawner`. Template-substitutes `command`. Participates in `tokio::select!` with `wait_for_kill` and an optional timeout.
- **`ScriptStep`**: Runs a script file via interpreter selected from `script_type`. Same PTY/timeout/kill machinery as `ShellStep`.
- **`HttpStep`**: Uses `reqwest`. Template-substitutes `url`, header values, and `body`. Validates response status against `expect_status`. Kill is implemented by dropping the in-flight future via `tokio::select!`.
- **`SetVarStep`**: Pure context mutation; no subprocess. Template-substitutes each `exports` value and inserts into `ctx.steps` as named exports. Never fails.
- **`MatchStep`**: Handled directly by `execute_steps()` in the executor rather than through the `Step` trait dispatch, because it needs to recursively call `execute_steps()` on its chosen branch.
- **`AgentStep`**: Resolves `AgentType` to an `AgentImpl` via `agents::impl_for()`, builds an argv array directly: `[claude, -p, <resolved_prompt>, --output-format, stream-json, --verbose, --dangerously-skip-permissions]`, with `--model <value>` inserted when `model` is set and `extra_args` appended verbatim. Spawns via `PtySpawner::spawn_argv` — no `cmd /C` / `sh -c` wrapper — which eliminates shell-escaping concerns for arbitrary prompt content. Streams output through `AgentOutputParser::parse_chunk()`. On completion calls `finalize()` to extract `AgentOutput` (cost, final message). Participates in kill/timeout select.

#### `workflow::agents` -- Agent Module

- **`AgentImpl` trait**: `fn build_argv(&self, prompt: &str, model: Option<&str>, extra_args: &[String]) -> Vec<String>; fn output_parser(&self) -> Box<dyn AgentOutputParser>`.
- **`AgentOutputParser` trait**: `fn parse_chunk(chunk: &[u8]); fn finalize(self: Box<Self>) -> AgentOutput`.
- **`AgentOutput`**: `cost: Option<CostFragment>`, `final_message: Option<String>`. Note: `CostFragment` is defined in `acs/src/workflow/step.rs`, not in `agents/mod.rs`.
- **`ClaudeCodeCli`** (`acs/src/workflow/agents/claude_code_cli.rs`): Builds argv: `[claude, -p, <prompt>, --output-format, stream-json, --verbose, --dangerously-skip-permissions]`, with `--model <val>` insertion point and `extra_args` appended. Parser (`ClaudeStreamParser`) buffers partial lines, processes `type=system` (model extraction) and `type=result` (cost, duration, turns, final message) NDJSON records, and accumulates totals across multiple invocations.

#### `workflow::log_sink` -- Log Sinks

- **`FileLogSink`** (`acs/src/workflow/log_sink.rs`): Writes to a single combined file per run at `<data_dir>/logs/<workflow_id>/<run_id>.log` in append mode. Emits versioned step-boundary markers:
  ```
  ===== ACS-<VERSION>:STEP:<step_id>:START:<iso8601> =====
  <stdout/stderr chunks>
  ===== ACS-<VERSION>:STEP:<step_id>:END:exit=<code>:<iso8601> =====
  ```
  `write_step_start` returns the byte offset before the marker; `write_step_end` returns the offset after.
- **`EventEmittingLogSink`** (`acs/src/workflow/event_log_sink.rs`): Wraps any `LogSink`. On `write_chunk`, also emits `WorkflowEvent::StepOutput` on the broadcast channel. Tracks `current_step` (index + id) via `set_current_step`, which the executor calls before each step. If `set_current_step` was never called, chunk events are silently skipped.

#### `pty` -- Process Spawning Abstraction

- **`PtySpawner` trait**: `fn spawn(cmd: CommandBuilder, rows: u16, cols: u16) -> anyhow::Result<Box<dyn PtyProcess>>`.
- **`PtyProcess` trait**: `read()`, `kill()`, `wait()`, `write_stdin()`, `close_stdin()`, `pid()`. `write_stdin()`/`close_stdin()`/`pid()` have default no-op/None implementations.
- **`NoPtySpawner`**: Production implementation using `std::process::Command` with piped stdout/stderr. On Windows uses `raw_arg()` to bypass Rust's MSVC quoting. On Unix uses `setsid()` to create a new process group (PGID == child PID). On Windows uses `CREATE_NEW_PROCESS_GROUP`. Both enable `kill_process_tree()` to target the full tree.
- **`MockPtySpawner`**: Test double with configurable output and exit codes.

#### `process_kill` -- Process Tree Termination

- **`kill_process_tree(pid)`** (`acs/src/process_kill.rs`): Gracefully terminates an entire process tree. On Unix: SIGTERM to process group, polls 5 s, escalates to SIGKILL. On Windows: delegates to `force_kill_process_tree` (no graceful equivalent).
- **`force_kill_process_tree(pid)`**: On Unix: SIGKILL to process group with fallback to single-PID kill. On Windows: `taskkill /T /F /PID`.

#### `errors` -- Error Types

- **`AcsError`**: `thiserror`-based enum with variants: `NotFound`, `Conflict`, `Validation`, `Storage`, `Internal`, `Cron`, `Pty`, `Timeout`. Implements `From<std::io::Error>`, `From<serde_json::Error>`, `From<uuid::Error>`.

#### `milepost` + `migrations` -- Data Migration (framework crate + ACS registry)

Since v5.0.0 the migration system is split like a package and its consumer:
`milepost/` (a sibling crate, consumed as a path dependency, versioned
independently) is a generic migration framework that knows nothing about
ACS; `acs/src/migrations/` owns the ACS migrations, the registry, and the
runner configuration.

- **`migrations::run_pending(data_dir)`** (`acs/src/migrations/mod.rs`): synchronous entry point the daemon calls at startup on a blocking task. Configures a `milepost::Runner` with ACS's registry, the `acs.db` path, a schema probe (`workflows` table exists), and the upgrade guidance for pre-tracking databases, then runs it. Execution is decided solely by the `schema_migrations` tracking table (no row = run; `success` = skip; `failed` = abort startup before anything runs). Each migration executes inside its own transaction; failures roll back completely, record a `failed` row with the error text, and abort. Rows for retired migrations (m001/m002 from v4 installs) are tolerated.
- **One migration kind**: every migration is a Rust file (`mNNN_<name>.rs`) implementing the `Migration` trait (`name()`, optional `baseline()` / `rebuild()` hooks, `up(&MigrationTx)`). The framework's `MigrationTx` is a small SQL-string API over the runner-owned transaction — `execute_batch(sql)`, `execute(sql, &[SqlValue])`, and `query(sql, &[SqlValue]) -> Vec<Vec<SqlValue>>` — so simple migrations are one SQL constant, and complex migrations (m005/m006, which need shell tokenizing and nested-JSON recursion) mix SQL strings with Rust-level logic.
- **Registry** (name-ordered): `m000_baseline` (baseline hook), `m003_drop_step_output_summary`, `m004_drop_input_schema`, `m005_shell_claude_to_agent` (SQL + Rust logic), `m006_agent_step_normalize` (SQL + Rust logic), `m007_add_token_columns`, `m008_add_workflow_deleted` (rebuild hook).
- **Conventions**: migrations whose `baseline()` hook returns true are recorded without executing on databases that already have migration tracking (every v4.2.14 install); migrations whose `rebuild()` hook returns true get `PRAGMA foreign_keys = OFF` around their transaction plus a pre-commit `PRAGMA foreign_key_check`. Databases without a tracking table must upgrade through v4.2.14 first.

See [Storage — Migration System](storage.md#9-migration-system) for full semantics.

---

## 3. Run Lifecycle

### 3.1 Startup Sequence

`start_daemon()` in `acs/src/daemon/mod.rs` orchestrates startup:

```
1.  load_config()                  — Load DaemonConfig (5-level resolution)
2.  Apply CLI overrides            — host_override, port_override
3.  resolve_data_dir()             — Determine data directory
4.  create_data_dirs()             — Ensure data/, data/logs/, data/scripts/ exist
5.  Set up tracing                 — Truncate daemon.log on startup, then open with
                                     SizeManagedWriter (auto-drops oldest 25% at 1 GB).
                                     Falls back to stderr-only on error.
6.  migrations::run_pending()      — Run pending migrations against the data
                                     directory (on a blocking task). Execution
                                     is tracked in the schema_migrations table;
                                     each migration runs in its own transaction
                                     and any failure aborts startup. Outcomes
                                     are written to daemon.log only (not
                                     /health).
7.  PidFile::acquire()             — Exclusive PID file (agentcronsystem.pid)
8.  sqlite::init_db()              — Open acs.db (apply pragmas + idempotent schema)
9.  SqliteWorkflowStore::new()     — Wrap the shared SqliteDb handle
9b. SqliteWorkflowRunStore::new()  — Same SqliteDb, run-store façade
10. broadcast::channel()           — Create WorkflowEvent bus (capacity from config)
11. Notify::new()                  — Create scheduler wake signal
12. watch::channel()               — Create shutdown signal
13. Build AppState                  — Aggregate all shared state + kill_signals registry
14. WorkflowScheduler::new()        — Create scheduler
15. tokio::spawn(scheduler.run())   — Start scheduler loop
16. TcpListener::bind()             — Bind HTTP server
17. PortFile::write_to()            — Write actual port to agentcronsystem.port
18. tokio::spawn(server)            — Start Axum server with graceful shutdown
19. Wait for signal                 — Ctrl+C, SIGTERM (Unix), or API shutdown
20. shutdown_tx.send()              — Signal HTTP server to stop
21. wf_scheduler_handle.abort()     — Stop scheduler
22. graceful_shutdown()             — Remove PID and port files
23. Await server_handle             — Wait for HTTP server to finish
```

### 3.2 Workflow Scheduling Flow

```
              WorkflowScheduler::run() loop
                         |
            1. workflow_store.list_workflows()
                         |
            2. Filter enabled workflows
                         |
            3. compute_next_run() for each
                         |
            4. Find earliest next_time
                         |
     +-------------------+--------------------+
     |                                        |
  tokio::time::sleep(duration)         notify.notified()
     |                                        |
  5. Re-check clock, for each due wf:   Re-loop from step 1
     a. If WaitForCompletion + active run:
        skip dispatch.
     b. Else if allow_concurrent=false +
        active run: skip dispatch and log
        a warning. Active run untouched.
     c. Generate run_id (Uuid::now_v7())
     d. tokio::spawn(async move {
          run_store.create_run(initial)
          create log_dir + FileLogSink
          wrap in EventEmittingLogSink
          build TriggerParams (empty input)
          run_workflow(wf, run_id, trigger,
            sink, event_tx,
            kill_signals=Some(registry))
          run_store.update_run(final_run)
        })
```

When the workflow list changes (create/update/delete via API), the route handler calls `scheduler_notify.notify_one()` to wake the scheduler.

Scheduler-dispatched runs register in the shared `kill_signals` registry, so `POST /api/runs/{id}/kill` works for cron-fired runs the same as for manually triggered runs.

### 3.3 Workflow Execution Flow (run_workflow)

`run_workflow()` in `acs/src/workflow/executor.rs`:

```
run_workflow(workflow, run_id, trigger, log_sink, event_tx, kill_signals)
    |
    1. Clone workflow as snapshot for WorkflowRun.workflow_snapshot
    2. Create watch::channel(false) for kill signal (KillSender/KillReceiver)
    3. Insert KillSender into kill_signals registry (if provided)
    4. Emit WorkflowEvent::RunStarted
    5. Resolve effective_input: trigger.input if not Null, else workflow.default_input
    6. Merge env: workflow.env_vars ← overlaid by trigger.env (trigger wins)
    7. Build StepContext { input, steps: {}, log_sink, env, working_dir,
                           event_tx, kill_rx }
    |
    execute_steps(workflow.steps, ..., &mut ctx, &mut step_runs,
                  &mut aborted, &mut killed)
        |
        For each StepDef:
          a. Check should_run: if aborted||killed, only run if always_run=true
          b. Increment ctx.step_index
          c. If MatchStep: evaluate expr template, look up branch, recurse;
             emit StepStarted + StepCompleted for synthetic MatchStep entry
          d. Else: emit StepStarted; call log_sink.set_current_step();
             run_step_with_policy(step_def, ctx, effective_policy, started_at)
               → dispatch_step() → step.execute(ctx)
             On Completed: insert StepOutput into ctx.steps; push StepRun
             On Failed (Abort): push StepRun; set aborted=true
             On Failed (Continue): insert output into ctx.steps; push StepRun
             On Killed: push StepRun; set killed=true; set aborted=true
             Emit StepCompleted event
    |
    8. Remove KillSender from registry (drops sender; receivers see RecvError)
    9. Determine final status: Killed > Failed (aborted) > Completed
    10. Sum total_cost_usd across AgentStep step runs
    11. Emit RunCompleted (or RunFailed for Failed/Killed)
    12. Return WorkflowRun { run_id, workflow_snapshot, steps, status,
                             total_cost_usd, total_duration_ms, ... }
```

### 3.4 Step Execution (Shell/Script)

Each `ShellStep::execute()` / `ScriptStep::execute()` follows a shared pattern:

```
1. Template-substitute command (or path + args)
2. Build CommandBuilder from command string
3. pty_spawner.spawn(cmd, rows, cols) → NoPtySpawner uses std::process::Command
   - Merge env: inherited < workflow.env_vars < trigger.env
   - Unix: setsid() creates new session with PGID == child PID
   - Windows: CREATE_NEW_PROCESS_GROUP; raw_arg() bypasses MSVC quoting
4. log_sink.write_step_start(step_id, started_at) → records byte offset
5. Create mpsc::channel(256) for output forwarding
6. tokio::task::spawn_blocking: read stdout/stderr in 8192-byte chunks,
   send via mpsc
7. Output forwarding loop (tokio::select!):
   chunk from output_rx → log_sink.write_chunk(data)
   wait_for_kill(kill_rx) → kill_process_tree(pid); return StepError::Killed
   timeout_fut (if timeout_secs set) → kill_process_tree(pid); return StepError::Timeout
8. Await read_handle (exit status)
9. log_sink.write_step_end(step_id, exit_code, finished_at) → records byte offset
10. Return StepOutput { exit_code, stdout: captured_output, exports: {}, cost: None }
```

### 3.5 Shutdown Sequence

Triggered by Ctrl+C, SIGTERM (Unix), or `POST /api/shutdown`:

```
1. shutdown_tx.send(())        — Signals HTTP server to stop accepting connections
2. wf_scheduler_handle.abort() — Stop scheduling new runs
3. graceful_shutdown():
   a. PidFile::release()       — Remove agentcronsystem.pid
   b. PortFile::remove()       — Remove agentcronsystem.port
4. Await server_handle         — Wait for HTTP server to finish
5. Exit with code 0
```

Note: in-flight workflow runs are not explicitly killed on shutdown. The kill-signals registry allows individual runs to be killed via `POST /api/runs/{run_id}/kill`, but graceful shutdown does not iterate the registry. In-flight processes will be terminated when the daemon process exits.

---

## 4. Data Model

### `Workflow` (`acs/src/models/workflow.rs`)

The only top-level scheduled entity. Owns schedule, steps, and runtime configuration.

| Field | Type | Notes |
|---|---|---|
| `id` | `Uuid` | UUIDv7, time-ordered |
| `name` | `String` | Unique slug |
| `version` | `u32` | Bumps on definition change |
| `schedule` | `String` | Cron expression |
| `timezone` | `Option<String>` | IANA timezone, e.g. `"America/New_York"` |
| `schedule_mode` | `ScheduleMode` | `Cron` (default) or `WaitForCompletion` |
| `enabled` | `bool` | Whether the scheduler fires this workflow |
| `steps` | `Vec<StepDef>` | Ordered list of step definitions |
| `default_input` | `Option<Value>` | Baseline input for cron-fired runs |
| `working_dir` | `Option<String>` | Workflow-level default for steps |
| `env_vars` | `Option<HashMap<String,String>>` | Workflow-level default |
| `allow_concurrent` | `bool` | Default `true`; set `false` to prevent parallel runs |
| `on_failure` | `FailurePolicy` | Workflow-level default applied to steps that don't specify their own |
| `last_run_at` / `last_run_status` / `last_run_id` | optional | Updated after each run |
| `next_run_at` | `Option<DateTime<Utc>>` | Computed, not persisted (`skip_deserializing`) |

### `StepDef` -- Step Variants

```
StepDef (tag = "kind")
├── Shell(ShellStep)       { common, command, pass_stdin }
├── Script(ScriptStep)     { common, path, script_type, args, pass_stdin }
├── Http(HttpStep)         { common, method, url, headers, body, expect_status }
├── Match(MatchStep)       { common, expr, cases: HashMap<String, Vec<StepDef>>, default }
├── SetVar(SetVarStep)     { common, exports: HashMap<String, String> }
└── Agent(AgentStep)       { common, agent_type, prompt, model, extra_args }
```

`StepDefCommon` (flattened into every variant): `id`, `on_failure`, `always_run`, `timeout_secs`, `working_dir`, `env_vars`, `capture: CaptureSpec { stdout_max_bytes, parser }`.

### `WorkflowRun` and `StepRun`

`WorkflowRun` is a complete, self-contained record of a single execution:

| Field | Notes |
|---|---|
| `workflow_snapshot` | Full `Workflow` definition copied at trigger time — runs are audit-complete without on-disk workflow file |
| `trigger_input` | Actual input used (after default vs. trigger replace) |
| `steps` | `Vec<StepRun>` in execution order (not definition order; `MatchStep` branches are flattened) |
| `total_cost_usd` | Sum of `AgentStep` costs; `None` if no agent steps ran |
| `total_duration_ms` | Wall-clock duration of the run |
| `total_input_tokens` | Sum of input tokens across `AgentStep` runs (from `usage.iterations[]` in the Claude CLI stream-json); `0` for non-agent runs. v4.2.11+. |
| `total_output_tokens` | Sum of output tokens across `AgentStep` runs; `0` for non-agent runs. v4.2.11+. |

`StepRun` per step:

| Field | Notes |
|---|---|
| `step_index` | 1-based position in the runtime execution sequence, matching the `step_index` in `StepStarted` / `StepCompleted` SSE events |
| `kind` | `"shell"` \| `"script"` \| `"http"` \| `"match"` \| `"set_var"` \| `"agent"` |
| `log_byte_offset_start` / `_end` | Byte range in the combined run log file for fast UI indexing. The captured stdout/stderr is only on disk — fetch the slice via `GET /api/runs/{run_id}/log?step_index=N`. |
| `cost_usd` | Populated only by `AgentStep` |

### `RunStatus`

`Running | Completed | Failed | Killed` (PascalCase in JSON via `#[serde(rename_all = "PascalCase")]`).

### `FailurePolicy`

`abort | continue | retry { attempts, backoff_ms }` (snake_case in JSON). Default is `abort`. `retry` exhaustion is treated as `abort`.

---

## 5. Concurrency

### 5.1 `allow_concurrent` Flag

`Workflow.allow_concurrent` is a universal concurrency guard that applies to all trigger sources (HTTP and cron).

- **`allow_concurrent: true`** (default): no concurrency check. Multiple runs of the same workflow may execute in parallel.
- **`allow_concurrent: false`** + active run for the workflow:
  - `POST /api/workflows/{id}/trigger` returns `409 Conflict` with body `{ "error": "concurrent_run_active", "message": "Workflow already has a running run; concurrent runs are disabled.", "active_run_id": "<run_id>" }`. No new run is created.
  - The cron tick is skipped and a warning is logged. The active run is left untouched and the next cron tick after it finishes is the one that will dispatch.
- **`schedule_mode: WaitForCompletion`** is independent of `allow_concurrent` and applies to cron only. When set, cron ticks are skipped while a run is active for the workflow regardless of the `allow_concurrent` value. HTTP triggers are unaffected by `schedule_mode`.

The dispatch decision for a cron tick is computed by `cron_dispatch_decision()` in `acs/src/daemon/scheduler.rs`, which returns one of `Dispatch`, `SkipWaitForCompletion`, or `SkipNoConcurrency`. `WaitForCompletion` takes precedence over `allow_concurrent: false` when both apply.

### 5.2 Kill Channel — `watch<bool>`

Kill signals use `tokio::sync::watch::channel(false)`:

- **Why `watch` (not `oneshot`)**: Multiple receivers can clone from a single sender (one per step in a multi-step run). The latest value is always immediately available via `borrow()`.
- **Per-run**: `run_workflow()` creates a `watch::channel(false)` at the start of each run. The `KillSender` is stored in `AppState.kill_signals` (an `Arc<RwLock<HashMap<Uuid, KillSender>>>`). The `KillReceiver` is cloned into each step's `StepContext`.
- **Signalling**: `POST /api/runs/{run_id}/kill` looks up the sender in the registry and calls `tx.send(true)`.
- **Step behavior**: Each subprocess step wraps its output loop in `tokio::select!` against `wait_for_kill(kill_rx)`. On kill, calls `kill_process_tree(pid)` and returns `StepError::Killed`.
- **Cleanup**: `run_workflow()` removes the sender from the registry after the run finishes. Dropping the sender signals any remaining receivers (they see `RecvError` on next `changed()` call).

### 5.3 Broadcast Channel -- Event Bus

```rust
let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(config.broadcast_capacity);
```

- **Capacity**: Configurable via `DaemonConfig::broadcast_capacity` (default 4096).
- **Producers**: `run_workflow()` (RunStarted, StepStarted, StepCompleted, RunCompleted, RunFailed), `EventEmittingLogSink` (StepOutput per chunk), API route handlers (WorkflowChanged).
- **Consumers**: SSE handler (`GET /api/events/workflows`), any subscriber via `event_tx.subscribe()`.
- **Backpressure**: Slow consumers receive `RecvError::Lagged(n)` and skip missed events.
- **`Arc<str>`**: `StepOutput.data` uses `Arc<str>` for zero-copy cloning.

### 5.4 Notify -- Scheduler Wake

`Arc<Notify>` wakes the `WorkflowScheduler` when the workflow list changes (create/update/delete/enable/disable). Route handlers call `scheduler_notify.notify_one()`.

### 5.5 Watch Channel -- Shutdown Signal

`tokio::sync::watch::channel(())` broadcasts the shutdown signal to the HTTP server's graceful shutdown handler (`with_graceful_shutdown`) and is also subscribed to by the main loop to detect API-initiated shutdowns.

### 5.6 Shared-State Primitives

Note: the SQLite stores share a single `Arc<std::sync::Mutex<rusqlite::Connection>>` exposed through `SqliteDb`. The `std::sync::Mutex` is correct because rusqlite is synchronous — calls happen inside `tokio::task::spawn_blocking` and never await while holding the lock.

| Resource | Type |
|---|---|
| `SqliteDb::conn` (shared by both stores) | `Arc<std::sync::Mutex<rusqlite::Connection>>` |
| `AppState::kill_signals` | `Arc<tokio::sync::RwLock<HashMap<Uuid, KillSender>>>` |

### 5.7 Blocking Work

PTY/process output reading uses `tokio::task::spawn_blocking()` because `ChildStdout::read()` is a blocking call. The blocking task sends chunks to the async side via `mpsc::channel(256)`.

---

## 6. Failure Model

### Step-Level

Each step's `on_failure` (or the workflow-level default `workflow.on_failure`) governs what happens when a step exits non-zero or returns `StepError`:

| Policy | Behavior |
|---|---|
| `Abort` (default) | Record `StepRun` as `Failed`, set `aborted=true`. Subsequent steps are skipped unless `always_run=true`. Run status becomes `Failed`. |
| `Continue` | Record `StepRun` as `Failed`, insert output into `ctx.steps` for downstream templates, continue to next step. Run status remains `Completed` if no `Abort`-policy steps also fail. |
| `Retry { attempts, backoff_ms }` | Retry up to `attempts` times with `backoff_ms` delay. Non-zero exits and `StepError` variants (except `Killed`) trigger retry. All retries exhausted → treated as `Abort`. `StepError::Killed` is always terminal. |

### `always_run`

Steps with `always_run: true` execute even when the run is in the `aborted` or `killed` state. Useful for cleanup/notification steps (analogous to `post_hook` in the pre-refactor model). Steps with `always_run: false` (default) are silently skipped after an abort.

### `StepError` Variants

`Spawn` (process could not start), `Io`, `Timeout(secs)`, `Template` (substitution failure), `Killed`, `Internal`. Non-zero exit codes are NOT `StepError` — they are returned as `StepOutput { exit_code: Some(non_zero) }` and the failure policy is then applied.

---

## 7. Process Spawning

### `NoPtySpawner` (production)

Uses `std::process::Command` with piped stdout and stderr (both merged into the read stream by the `PtyProcess` implementation). Piped I/O reliably delivers EOF on all platforms.

**Unix**: New session via `setsid()`, making PGID == child PID. Allows `killpg(pid, signal)` to terminate the entire process tree.

**Windows**: `CREATE_NEW_PROCESS_GROUP` flag. `raw_arg()` bypasses Rust's MSVC quoting so `cmd.exe /C <command>` receives the string verbatim.

### `kill_process_tree(pid)` (`acs/src/process_kill.rs`)

Async function used by all subprocess step impls when a kill or timeout fires:
- **Unix**: SIGTERM to process group → poll 5 s (50 × 100 ms) → SIGKILL if still alive.
- **Windows**: delegates to `force_kill_process_tree` immediately (`taskkill /T /F /PID`).

### `MockPtySpawner` (tests)

Test double with configurable output bytes and exit code, used by all step unit tests.

---

## 8. Storage Layout

On-disk layout under `data_dir` (default: platform data dir / `agent-cron-scheduler`):

```
data_dir/
  agentcronsystem.pid          # Exclusive PID file
  agentcronsystem.port         # Bound port (written after bind, removed on shutdown)
  daemon.log                   # Daemon tracing output; auto-truncated at 1 GB
  config.json                  # Optional; loaded if present
  acs.db                       # SQLite database (workflows + workflow_runs tables)
  acs.db-wal                   # SQLite write-ahead log (managed by SQLite)
  acs.db-shm                   # SQLite shared memory file (managed by SQLite)
  logs/
    <workflow_id>/
      <run_id>.log             # Combined step output with ACS marker lines
  scripts/                     # User script files (referenced by ScriptStep.path)
```

The log file format uses versioned step-boundary markers:
```
===== ACS-<VERSION>:STEP:<step_id>:START:<iso8601> =====
<stdout/stderr interleaved>
===== ACS-<VERSION>:STEP:<step_id>:END:exit=<code>:<iso8601> =====
```

`StepRun.log_byte_offset_start` and `.log_byte_offset_end` index into the combined log for fast per-step log retrieval in the UI.

Cross-reference: [Storage](storage.md).

---

## 9. Migration

Since v5.0.0 the migration framework lives in the `milepost/` sibling crate
(a generic library that knows nothing about ACS), the migrations themselves
live in `acs/src/migrations/`, and execution is tracked in the
`schema_migrations` table inside `acs.db` (the legacy
`migrations.json` state file and the v4-era m001/m002 Rust migrations were
retired; databases that predate the tracking table must upgrade through
v4.2.14 first).

Highlights:

- **One migration kind**: Rust files implementing the `Migration` trait,
  with SQL kept in string constants and executed through the framework's
  `MigrationTx` SQL-string API (`execute_batch` / `execute` / `query` over
  plain `SqlValue` rows). Complex migrations (m005/m006) mix SQL strings
  with Rust-level logic that SQL alone cannot express. Same registry, same
  ordering, same tracking, same per-migration transaction and fail-stop
  semantics for every migration.
- **Baseline**: `m000_baseline` creates the pre-m003-era schema on fresh
  installs, then m003–m008 apply on top; on any database that already has
  migration tracking, the baseline is recorded without executing (the
  trait's `baseline()` hook).
- **Rebuild convention**: migrations opting in via the trait's `rebuild()`
  hook (m008) get `PRAGMA foreign_keys = OFF` around their transaction and
  a pre-commit `PRAGMA foreign_key_check`.
- **Failure semantics**: each migration runs in its own transaction; a
  failure rolls back, records a `failed` row with the error text, and aborts
  startup. Recovery = fix the issue, `DELETE` the row, restart.

Full documentation, including per-migration history: see
[Storage — Migration System](storage.md#9-migration-system).

---

## 10. Testing Strategy

### Unit Tests (in-module, `cargo test --lib`)

- **`daemon/mod.rs`**: PidFile acquire/release/stale-detection, PortFile, `graceful_shutdown`, `create_data_dirs`, `load_config`, `resolve_data_dir`, `SizeManagedWriter` truncation behavior.
- **`daemon/scheduler.rs`**: `compute_next_run()` with UTC and timezone; DST spring-forward and fall-back; `FakeClock` behavior.
- **`daemon/events.rs`**: `WorkflowEvent` serde round-trips per variant, `Arc<str>` cheap clone, broadcast send/receive/lag.
- **`workflow/executor.rs`**: Multi-step happy path, abort-on-failure (skips subsequent), `always_run` cleanup after abort, Continue policy, MatchStep happy path / default branch / no-match no-op, HttpStep dispatched, AgentStep dispatched, default_input application, trigger input override, env overlay, kill signal aborts long-running step, kill only affects target run, kill after completion is no-op.
- **`workflow/template.rs`**: `input.*` substitution (flat, nested, missing), `steps.*.*` (stdout string, stdout structured, exit_code, exports), edge cases (unclosed brace, empty path, missing step, literal `$` without brace).
- **`workflow/log_sink.rs`**: Marker ordering, byte offsets, binary chunk preservation, `None` exit code renders as `-1`, sequential offsets across steps.
- **`workflow/event_log_sink.rs`**: `set_current_step` propagates to chunk events, chunk includes run_id + workflow_id, no panic on zero subscribers, chunk before `set_current_step` emits nothing, inner sink still receives chunks, `set_current_step` overwrite, forwarded to inner.
- **`workflow/steps/shell.rs`** and **`script.rs`**: Cross-platform (`#[cfg(unix)]` + `#[cfg(windows)]` mirrors). Tests cover exit-code capture, output capture, timeout, env vars, template substitution, pass_stdin.
- **`storage/sqlite/workflows.rs`**: CRUD round-trips, `find_by_name`, not-found, conflict (UNIQUE on name), version-bump rules, partial updates.
- **`storage/sqlite/workflow_runs.rs`**: create/update/get/list/count/delete via the FK-enforced `workflow_runs` table, latest-first ordering by `run_id DESC`, pagination, FK to `workflows.id`.
- **`storage/sqlite/schema.rs`**: pragma + schema idempotency, UNIQUE constraint enforcement.
- **`migration/mod.rs`**: State read/write round-trip, corruption tolerance, all-migrations run on fresh install, already-applied skipped, not-needed goes to `skipped_not_needed`, stops at first failure, idempotent, atomic write (no `.tmp` leftover), real-registry smoke test.
- **`errors.rs`**: Display formatting for all `AcsError` variants, `From<>` impls.
- **`process_kill.rs`**: Kill of dead PID does not panic (both `kill_process_tree` and `force_kill_process_tree`).

### Integration Tests (`cargo test --tests`)

Integration tests in `acs/tests/workflow_api_tests.rs` (~70 cases) and `acs/tests/cli_tests.rs` (~9 cases). Run `cargo test` for the authoritative count.

---

## 11. Key Design Decisions

### 11.1 Workflow-Only Model

There is no separate `Job` concept. A `Workflow` owns its schedule, steps, and all runtime configuration. The pre-refactor `pre_hook`/`post_hook` model is superseded by `always_run` steps, which are explicit in the workflow definition and visible in run records.

### 11.2 Workflow Snapshot in Run Records

`WorkflowRun.workflow_snapshot` is a full copy of the `Workflow` at trigger time. Runs are self-contained for audit and replay. `GET /api/runs/{id}` never depends on the current on-disk workflow file. This also permits one-off ad-hoc runs via `POST /api/workflows/{id}/trigger` with inline definition overrides.

### 11.3 Trait-Based Abstractions

Storage (`WorkflowStore`, `WorkflowRunStore`), process spawning (`PtySpawner`, `PtyProcess`), step execution (`Step`), log writing (`LogSink`), agent I/O (`AgentImpl`, `AgentOutputParser`), and time (`Clock`) are all behind traits with `Arc<dyn ...>`. This decouples business logic from implementation, enables in-memory/mock test doubles, and keeps the extension surface minimal (new agent = one new file + one enum variant).

### 11.4 Streaming Cost Extraction

Agent cost data is extracted inline during the step's output streaming via `AgentOutputParser::parse_chunk()`, not post-hoc from logs. `ClaudeStreamParser` (`acs/src/workflow/agents/claude_code_cli.rs`) buffers partial NDJSON lines, processes `type=system` and `type=result` records, and accumulates totals. A `ShellStep` that happens to call `claude -p` directly does not get cost tracking — cost tracking is opt-in via `AgentStep`.

Alongside `total_cost_usd`, the same parser also sums input/output token counts across the Claude CLI `usage.iterations[]` array. Per-step totals are aggregated into `WorkflowRun.total_input_tokens` / `total_output_tokens` at run completion (non-agent steps contribute 0) and surfaced by the cost API on `CostSummary` (rolling 30-day and 1-year windows) and on each `DailyBucket` (per-status splits plus cross-status totals). Added in v4.2.11.

### 11.5 EventEmittingLogSink Decorator

Real-time stdout streaming to SSE clients is implemented as a `LogSink` wrapper (`EventEmittingLogSink`) rather than a parallel channel in each step implementation. Steps write to `LogSink` normally; the wrapper intercepts `write_chunk` calls and emits `WorkflowEvent::StepOutput` on the broadcast channel. `set_current_step()` propagates the active step's index and id so each chunk event is correctly tagged.

### 11.6 PID File Locking

Single-instance enforcement uses `create_new(true)` (`O_EXCL`/`CREATE_NEW`). Stale PID files (process dead) are removed and re-acquired. Live conflicts retry for up to 10 s (20 × 500 ms) to tolerate restart overlap. Windows adds a `GetExitCodeProcess` check because `OpenProcess` can succeed on a dead process that still has a handle open (zombie-handle scenario observed in production).

### 11.7 Piped I/O over PTY

`NoPtySpawner` uses `std::process::Command` with piped stdout/stderr rather than a real PTY. Both streams are merged into a single read stream by the `PtyProcess` implementation. Piped I/O reliably delivers EOF on all platforms and avoids platform-specific PTY issues. Each child process is placed in a new process group (Unix: `setsid()`; Windows: `CREATE_NEW_PROCESS_GROUP`) to enable tree-wide kill.

### 11.8 Atomic File Persistence

Structured persistence (workflows + run records) lives in `acs.db`, a SQLite database opened in WAL mode with `synchronous=NORMAL`. Every mutation runs inside a transaction, so a crash mid-write either commits the full change or none of it. `SizeManagedWriter` (daemon.log) still uses write-to-temp-then-rename for the file-based artefacts that remain outside the database. Migration state lives in the `schema_migrations` table inside `acs.db`, covered by the same transactional guarantees.

### 11.9 Numbered Migration System

Migrations are forward-only, name-ordered entries in ACS's registry (`acs/src/migrations/`), run through the generic `milepost` framework crate — Rust files whose SQL lives in string constants, executed through the framework's SQL-string API, with Rust-level logic only where SQL cannot express the transform. Execution state lives in the `schema_migrations` table inside `acs.db` and is recorded after each migration, so progress is preserved on partial failure. Each migration runs in its own transaction; a failure rolls back, records a `failed` row, and blocks startup until an operator deletes the row.
