# Storage and Data Management

This document describes how the Agent Cron Scheduler (ACS) persists workflows,
run records, logs, daemon state, and configuration on disk.  All paths below
are relative to the **data directory** (`{data_dir}`).

For how the data directory is resolved (CLI flags, env vars, platform defaults),
see [Configuration](configuration.md#data-directory-locations).

---

## 1. Overview

Storage is organised around two complementary concerns:

* **Trait-based stores** — all persistence goes through an `async_trait`
  interface.  Each trait has one SQLite-backed implementation (`Sqlite*`) and
  can be replaced by an in-memory mock for tests.
* **Single rooted layout** — every file the daemon writes lives under a single
  `data_dir` chosen at startup.  There is no global state outside that tree.

The three active store traits are:

| Trait | Impl | What it stores |
|---|---|---|
| `WorkflowStore` | `SqliteWorkflowStore` | Workflow definitions (`workflows` table in `acs.db`) |
| `WorkflowRunStore` | `SqliteWorkflowRunStore` | `WorkflowRun` records (`workflow_runs` table in `acs.db`) |
| *(daemon)* | `SizeManagedWriter` | Daemon process log (`daemon.log`) |

Step output is written by `FileLogSink` (not through a store trait) to per-run
files under `logs/`.  Migration execution is tracked in the
`schema_migrations` table inside `acs.db`, owned by the migration runner
(see [Migration System](#9-migration-system)).

---

## 2. Data Directory Layout

```
{data_dir}/
├── agentcronsystem.pid         # Daemon PID file (exclusive creation prevents duplicate instances)
├── agentcronsystem.port        # TCP port the daemon is listening on
├── config.json                 # Daemon config (fallback location; see configuration.md)
├── daemon.log                  # Daemon process log (size-managed, max 1 GB)
├── acs.db                      # SQLite database holding workflows + workflow_runs tables
├── acs.db-wal                  # SQLite write-ahead log (created at runtime, managed by SQLite)
├── acs.db-shm                  # SQLite shared memory file (created at runtime, managed by SQLite)
├── migrations.json             # Inert v4-era leftover, ignored by v5 (may exist on upgraded installs)
├── jobs.json.migrated.<ts>     # Inert v4-era backup of legacy jobs.json (may exist on upgraded installs)
├── migrated_scripts/           # Inert v4-era migrated hook scripts (may exist on upgraded installs)
├── scripts/                    # Reserved directory (created on startup; not currently used)
└── logs/
    └── {workflow_id}/          # One directory per workflow, named by UUID
        └── {run_id}.log        # Combined step output for a single run (append-only)
```

On daemon startup, `create_data_dirs()` ensures the top-level directory and
the `logs/` and `scripts/` subdirectories exist.  The `acs.db` file is created
by the migration runner (to host its `schema_migrations` tracking table)
before any migration runs; the baseline and subsequent migrations then bring
it to the current schema, and `init_db` re-applies the idempotent schema
statements when the daemon opens it.  The `acs.db-wal` and
`acs.db-shm` sidecar files are created by SQLite the first time the database
is opened in WAL mode and persist alongside the DB.

---

## 3. SQLite Database

**Sources:** `acs/src/storage/sqlite/mod.rs`,
`acs/src/storage/sqlite/schema.rs`,
`acs/src/storage/sqlite/workflows.rs`,
`acs/src/storage/sqlite/workflow_runs.rs`

A single SQLite database at `{data_dir}/acs.db` holds all structured
persistence.  Both trait implementations share one `SqliteDb` handle, which
wraps `Arc<Mutex<rusqlite::Connection>>`.  Every async trait method offloads
rusqlite work via `tokio::task::spawn_blocking` so the runtime is never
blocked by synchronous DB calls.

### Pragmas

Applied on every connection by `apply_pragmas()`:

| Pragma | Value | Reason |
|---|---|---|
| `journal_mode` | `WAL` | Readers and writers do not block each other; better concurrency than the default rollback journal. |
| `foreign_keys` | `ON` | Actually enforce the FK on `workflow_runs.workflow_id`. SQLite has FK disabled by default. |
| `synchronous` | `NORMAL` | Safe under WAL (writes are still durable after commit) and noticeably faster than `FULL`. |

### Schema

Created idempotently by `apply_schema()` (every `CREATE TABLE` /
`CREATE INDEX` is `IF NOT EXISTS`):

```sql
CREATE TABLE workflows (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,  -- uniqueness via idx_workflows_name_live (live rows only, v4.2.14)
    version             INTEGER NOT NULL,
    schedule            TEXT NOT NULL,
    timezone            TEXT,
    schedule_mode       TEXT NOT NULL,
    enabled             INTEGER NOT NULL,
    steps_json          TEXT NOT NULL,
    default_input       TEXT,
    working_dir         TEXT,
    env_vars            TEXT,
    allow_concurrent    INTEGER NOT NULL,
    on_failure          TEXT NOT NULL,
    last_run_at         TEXT,
    last_run_status     TEXT,
    last_run_id         TEXT,
    created_at          TEXT NOT NULL,
    updated_at          TEXT NOT NULL,
    is_favorited        INTEGER NOT NULL DEFAULT 0,
    deleted             INTEGER NOT NULL DEFAULT 0   -- soft-delete flag; added by m008 (v4.2.14)
);

-- Name uniqueness among LIVE workflows only (v4.2.14): soft-deleted
-- rows are exempt, so a name becomes reusable after its owner is deleted
-- while name resolution stays unambiguous.
CREATE UNIQUE INDEX idx_workflows_name_live
    ON workflows(name) WHERE deleted = 0;

CREATE TABLE workflow_runs (
    run_id              TEXT PRIMARY KEY,
    workflow_id         TEXT NOT NULL,
    workflow_version    INTEGER NOT NULL,
    workflow_snapshot   TEXT NOT NULL,
    started_at          TEXT NOT NULL,
    finished_at         TEXT,
    status              TEXT NOT NULL,
    trigger_input       TEXT,
    steps_json          TEXT NOT NULL,
    total_cost_usd      REAL,
    total_duration_ms   INTEGER,
    total_input_tokens  INTEGER NOT NULL DEFAULT 0,  -- added by m007 (v4.2.11)
    total_output_tokens INTEGER NOT NULL DEFAULT 0,  -- added by m007 (v4.2.11)
    FOREIGN KEY (workflow_id) REFERENCES workflows(id)
);

CREATE INDEX idx_workflow_runs_workflow_id_finished_at
    ON workflow_runs(workflow_id, finished_at);
CREATE INDEX idx_workflow_runs_finished_at
    ON workflow_runs(finished_at);
CREATE INDEX idx_workflow_runs_status
    ON workflow_runs(status);

CREATE TABLE meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
```

### Encoding rules

- UUIDs (`id`, `run_id`, `workflow_id`, `last_run_id`) are stored as their
  RFC 4122 hyphenated TEXT form.
- Booleans (`enabled`, `allow_concurrent`) are `INTEGER` (`0` / `1`).
- Timestamps are RFC 3339 TEXT (`DateTime<Utc>::to_rfc3339()`).
- `schedule_mode` is the bare snake_case enum string (e.g. `"cron"`).
- `last_run_status` and `workflow_runs.status` are the bare PascalCase
  `RunStatus` strings (`"Running"`, `"Completed"`, `"Failed"`, `"Killed"`).
- `on_failure` is the full JSON serialisation of `FailurePolicy` because the
  `Retry { attempts, backoff_ms }` variant carries data and cannot be a bare
  string.
- `steps_json` (workflows) is `Vec<StepDef>` serialised; `steps_json`
  (workflow_runs) is `Vec<StepRun>` serialised.
- `workflow_snapshot` is the full `Workflow` definition at trigger time
  serialised as JSON, so each run record is self-contained.
- `default_input`, `trigger_input`, and `env_vars` are optional JSON TEXT
  blobs (`NULL` when absent).

### Atomicity and durability

Every mutation runs inside a SQLite transaction.  Trait methods that perform
multi-step changes (e.g., `update_workflow` reads, mutates, and writes back)
take a `&mut Connection` for the duration of the operation, so the read and
write are part of the same logical unit.  WAL mode plus `synchronous = NORMAL`
guarantees that committed transactions survive process crashes; the WAL is
checkpointed back into the main file periodically and on connection close.

### Concurrency

The shared `Mutex<Connection>` serialises calls into rusqlite within the
process.  WAL mode means external readers (e.g., a `sqlite3` shell opened
against the same file for inspection) do not block the daemon and vice
versa.

---

## 4. Corruption Handling

### `acs.db`

If `init_db()` fails to open `acs.db` (e.g., the file is missing or the
header is corrupt), daemon startup aborts with a non-zero exit code; no data
is silently dropped.  Recovery is operator-driven: restore from a known-good
backup of the data directory.  Because every transaction is atomic, partial
writes from a crash never leave the database structurally inconsistent.

### Migration tracking state

Migration execution is tracked in the `schema_migrations` table inside
`acs.db`, protected by the same SQLite atomicity guarantees as the data
tables.  The table is the sole source of truth; there is no auxiliary state
file.  (The legacy `migrations.json` backfill shipped by v4.2.x was retired
in v5.0.0; databases whose schema has no recorded migration history are
rejected — see [Migration System](#9-migration-system).)


---

## 5. WorkflowStore

**Sources:** `acs/src/storage/mod.rs`, `acs/src/storage/workflows.rs`,
`acs/src/storage/sqlite/workflows.rs`

### Trait

```rust
#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn list_workflows(&self) -> Result<Vec<Workflow>>;
    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>>;
    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow>;
    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow>;
    async fn delete_workflow(&self, id: Uuid) -> Result<()>;
    async fn record_run_outcome(
        &self,
        workflow_id: Uuid,
        run_id: Uuid,
        status: RunStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<()>;
}
```

| Method | Description |
|---|---|
| `list_workflows` | Returns all workflows ordered by `created_at ASC`. |
| `get_workflow` | Looks up a single workflow by UUID; returns `None` if not found. |
| `find_by_name` | Looks up a single workflow by name; returns `None` if not found. |
| `create_workflow` | Validates, assigns a UUIDv7 ID, sets `version: 1`, INSERTs, and returns the new workflow. |
| `update_workflow` | Partial update; bumps `version` when any definition-affecting field changes. Returns `NotFound` or `Conflict` as appropriate. |
| `delete_workflow` | DELETEs a workflow by UUID; returns `NotFound` if it does not exist. |
| `record_run_outcome` | Records a terminal run outcome on the parent workflow (updates `last_run_id`, `last_run_status`, `last_run_at`; bumps `updated_at`; does NOT bump `version`). Returns `Result<()>` (anyhow); note this differs from the `WorkflowRunStore` trait which uses `Result<_, AcsError>`. |

### SqliteWorkflowStore

```rust
pub struct SqliteWorkflowStore {
    db: SqliteDb,
}
```

The store is a thin wrapper around the shared `SqliteDb`.  `list_workflows`
issues a single `SELECT *` and maps each row through `row_to_workflow`.
Mutations go through prepared INSERT / UPDATE statements and translate the
SQLite `UNIQUE` constraint violation on `workflows.name` to
`AcsError::Conflict`.

### Version bump rules

`update_workflow` tracks whether any **definition-affecting field** changed.
Definition-affecting fields are: `steps`, `on_failure`, `default_input`,
`working_dir`, `env_vars`, `allow_concurrent`, `schedule`, `schedule_mode`,
`timezone`, and `name`.

The `enabled` flag is explicitly excluded — toggling a workflow on or off does
not alter its definition and therefore does **not** bump `version`.

Runtime metadata fields (`last_run_at`, `last_run_status`, `last_run_id`) are
not present in `WorkflowUpdate` at all and cannot trigger a version bump.

### Duplicate name enforcement

`create_workflow` relies on the `UNIQUE` constraint on `workflows.name`;
violations are mapped to `AcsError::Conflict`.  `update_workflow` performs an
explicit `SELECT COUNT(*) … WHERE name = ? AND id != ?` check before issuing
the UPDATE so the conflict path is symmetric.

---

## 6. WorkflowRunStore

**Sources:** `acs/src/storage/workflow_runs.rs`,
`acs/src/storage/sqlite/workflow_runs.rs`

### Trait

```rust
#[async_trait]
pub trait WorkflowRunStore: Send + Sync {
    async fn create_run(&self, run: WorkflowRun) -> Result<(), AcsError>;
    async fn update_run(&self, run: &WorkflowRun) -> Result<(), AcsError>;
    async fn get_run(&self, run_id: Uuid) -> Result<Option<WorkflowRun>, AcsError>;
    async fn list_runs(
        &self,
        workflow_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowRun>, AcsError>;
    async fn count_runs(&self, workflow_id: Uuid) -> Result<usize, AcsError>;
    async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError>;
    async fn list_recent_runs(&self, limit: usize, offset: usize) -> Result<Vec<WorkflowRun>, AcsError>;
    async fn count_all_runs(&self) -> Result<usize, AcsError>;
    async fn cost_summary_for(&self, workflow_id: Uuid, display_tz: &Tz) -> Result<CostSummary, AcsError>;
    async fn daily_buckets_for(&self, workflow_id: Option<Uuid>, since: DateTime<Utc>, until: DateTime<Utc>, display_tz: &Tz) -> Result<Vec<DailyBucket>, AcsError>;
}
```

| Method | Description |
|---|---|
| `create_run` | INSERTs the initial run record. |
| `update_run` | UPSERTs (`INSERT … ON CONFLICT(run_id) DO UPDATE`); returns `NotFound` if the row is not already present. |
| `get_run` | Single-row SELECT by primary key; returns `None` if the row is absent. |
| `list_runs` | `SELECT … WHERE workflow_id = ? ORDER BY run_id DESC LIMIT ? OFFSET ?`. `limit=0` is translated to `-1` (SQLite "no limit"). |
| `count_runs` | `SELECT COUNT(*) … WHERE workflow_id = ?`. |
| `delete_run` | `DELETE FROM workflow_runs WHERE run_id = ?` (best-effort; absent rows are not an error). Log files are not removed by `delete_run`. |
| `list_recent_runs` | Cross-workflow recent-runs feed: `SELECT … ORDER BY run_id DESC LIMIT ? OFFSET ?` (no `workflow_id` filter). Backs `GET /api/runs/recent`. |
| `count_all_runs` | `SELECT COUNT(*) FROM workflow_runs` across all workflows. Pairs with `list_recent_runs` for paginated totals. |
| `cost_summary_for` | Computes the per-workflow `CostSummary` (30-day + 1-year totals) windowed against `display_tz` calendar-day boundaries. Entry point used by `CostCache`. |
| `daily_buckets_for` | Per-day cost buckets over the `[since, until)` window for a single workflow when `workflow_id` is `Some`, or system-wide aggregate when `None`. Results are in ascending date order; days with no terminal runs are omitted. Date grouping uses `display_tz` for calendar-day boundaries. |

### SqliteWorkflowRunStore

```rust
pub struct SqliteWorkflowRunStore {
    db: SqliteDb,
}
```

Like `SqliteWorkflowStore`, this is a thin wrapper around the shared
`SqliteDb`.  The `INSERT … ON CONFLICT … DO UPDATE` pattern in `upsert_run`
keeps `create_run` and `update_run` symmetric: both end up writing a row
keyed by `run_id`, and `update_run` only differs in that it asserts the row
already exists.

### Latest-first ordering

`list_runs` orders by `run_id DESC`.  Run IDs are UUIDv7 values, which are
monotonically time-ordered, so descending lexicographic order is equivalent
to latest-first chronological order without any additional join or
secondary sort.

### Indexes

The `workflows` table has one secondary index and the `workflow_runs` table
has three:

| Index | Columns | Purpose |
|---|---|---|
| `idx_workflows_name_live` | `(name) WHERE deleted = 0` (partial, unique) | Name uniqueness among **live** workflows only; soft-deleted rows are exempt so names are reusable after deletion (v4.2.14). |
| `idx_workflow_runs_workflow_id_finished_at` | `(workflow_id, finished_at)` | Per-workflow listings ordered by completion time. |
| `idx_workflow_runs_finished_at` | `(finished_at)` | Cross-workflow recency queries. |
| `idx_workflow_runs_status` | `(status)` | Filters that pick out e.g. all currently-running rows. |

The cost-analytics aggregation query — `SUM(CASE WHEN finished_at >= ? THEN total_cost_usd END)` plus `COUNT(...)` over two windows — runs once per workflow per `GET /api/cost/workflows[/{id}]` cache miss. The `idx_workflow_runs_workflow_id_finished_at` composite index established in the baseline schema covers this access pattern: the WHERE clause filters by `workflow_id`, `status IN (...)`, and `finished_at >= ?`, with the conditional SUMs evaluated over the filtered rows.

The daily-bucket query fetches `(finished_at, status, total_cost_usd, total_input_tokens, total_output_tokens)` over the window for the requested workflow (or all workflows for the system aggregate). Per-day grouping happens in Rust using `chrono_tz` to convert each UTC `finished_at` to the daemon's `display_timezone` local date — this avoids fighting SQLite's `localtime` / `strftime` for cross-platform consistency. The same `idx_workflow_runs_workflow_id_finished_at` composite index covers both per-workflow and system-wide queries. Both query paths are invoked exclusively by the cost endpoint handlers at `GET /api/cost/workflows[/{id}]`. The `total_input_tokens` / `total_output_tokens` columns were added in v4.2.11 (migration m007).

---

## 7. FileLogSink

**Source:** `acs/src/workflow/log_sink.rs`

`FileLogSink` is the concrete [`LogSink`] trait implementation that writes step
output to a single combined log file per run.  It is **not** accessed through
`WorkflowRunStore`; the daemon creates it directly and passes it to the
workflow executor as an `Arc<dyn LogSink>`.

### File location

```
{data_dir}/logs/{workflow_id}/{run_id}.log
```

The file is opened in **create + append** mode.  If the file already exists
(e.g., the daemon resumed after a crash), new output is appended after existing
content.  The initial byte offset is seeded from `metadata().len()` so position
tracking remains accurate.

### Marker format

At the start and end of each step the executor calls `write_step_start` and
`write_step_end`, which write delimiter lines that identify the step, the
daemon version, and a timestamp:

```
===== ACS-<VERSION>:STEP:<step_id>:START:<iso8601> =====
<step stdout and stderr interleaved>
===== ACS-<VERSION>:STEP:<step_id>:END:exit=<code>:<iso8601> =====
```

`<VERSION>` is the value of the `CARGO_PKG_VERSION` environment variable
compiled into the binary (e.g. `0.4.1`).  The version stamp lets log parsers
handle format changes across daemon releases.

When the exit code is unavailable (e.g., the step was killed before it could
return), the code field is rendered as `-1`:

```
===== ACS-0.4.1:STEP:main:END:exit=-1:2026-05-02T14:30:00Z =====
```

### Byte-offset tracking

`write_step_start` returns the file offset **before** the start marker is
written.  `write_step_end` returns the file offset **after** the end marker is
written.  The executor stores these offsets in `StepRun.log_byte_offset_start`
and `StepRun.log_byte_offset_end` respectively, enabling fast random access to
any step's output without scanning the entire file.

`log_byte_offset_end` is populated on every exit path that reached the run
loop, including kill, timeout, IO error, and non-zero exit, because the step
impls write the END marker before surfacing the error. The only cases where
`log_byte_offset_end` stays `null` are:

* The step is still running (the live tail case).
* The step erred **before** `write_step_start` ran — for example a
  template-substitution failure or a spawn failure where no run loop ever
  started. In that scenario `log_byte_offset_start` is also unset and the
  slice endpoint falls back to `0` for the start and "tail to EOF" for the end.

Per-step output lives only in this log file — the SQLite `workflow_runs.steps_json`
blob carries the byte offsets but not the bytes themselves. Clients fetch step
output via `GET /api/runs/{run_id}/log?step_index=N`, which seeks to
`log_byte_offset_start` and reads through `log_byte_offset_end` (or end-of-file
when `_end` is `null`).

---

## 8. EventEmittingLogSink

**Source:** `acs/src/workflow/event_log_sink.rs`

`EventEmittingLogSink` is a `LogSink` wrapper that delegates all method calls
to an inner `Arc<dyn LogSink>` while also emitting `WorkflowEvent::StepOutput`
SSE events for every `write_chunk` call.  It has **no on-disk effect of its
own** — it writes nothing directly to disk.

```rust
pub struct EventEmittingLogSink {
    inner: Arc<dyn LogSink>,
    event_tx: broadcast::Sender<WorkflowEvent>,
    run_id: Uuid,
    workflow_id: Uuid,
    current_step: Mutex<Option<CurrentStep>>,
}
```

### Wiring

Both the trigger handler and the cron scheduler wrap `FileLogSink::create(...)`
in `EventEmittingLogSink::new(...)` before passing the sink to `run_workflow`.
The inner `FileLogSink` handles persistence; the wrapper adds the live-stream
layer.

### set_current_step

The executor calls `set_current_step(step_index, step_id)` before invoking
each step's `execute()`.  This updates the `current_step` field so that
subsequent `write_chunk` calls emit events tagged with the correct
`step_index` and `step_id`.  The call is forwarded to the inner sink (which
ignores it by default via the `LogSink` trait's no-op implementation).

### Chunk events

On every `write_chunk(data)`:
1. If `set_current_step` has been called, a `WorkflowEvent::StepOutput` event
   is sent on the broadcast channel with `run_id`, `workflow_id`, `step_index`,
   `step_id`, the chunk data (lossy UTF-8), and a timestamp.
2. If no subscribers are listening (the broadcast send returns an error), the
   error is silently discarded — this never blocks or panics.
3. The chunk is always forwarded to the inner sink regardless of whether an
   event was emitted.

---

## 9. Migration System

**Sources:** `milepost/src/lib.rs` (framework),
`acs/src/migrations/mod.rs` (registry + runner configuration),
`acs/src/migrations/m*.rs`, `acs/src/migrations/shell_tokens.rs`

### Design (v5.0.0): the `milepost` framework + ACS-owned migrations

Since v5.0.0 the migration system is split like a package and its consumer:

* **`milepost/`** is a generic, reusable migration framework — a sibling
  crate consumed as a path dependency, versioned independently (0.1.0). It
  contains ONLY framework functionality (the `Migration` trait, the
  `MigrationTx` SQL-string API, the runner, tracking-table management) and
  knows nothing about ACS.
* **`acs/src/migrations/`** owns everything ACS-specific: the migration
  files, the registry, and the runner configuration — which database file
  to migrate and the schema probe (an ACS database is recognised by its
  `workflows` table).

The separation exists for maintenance, clarity, and future development: the
framework and the shipped migrations evolve, get reviewed, and get tested
independently, and nothing in the framework can reach into ACS's live model
types.  The daemon calls `acs::migrations::run_pending(data_dir)` at startup
(on a blocking task), which configures a `milepost::Runner` and executes it
before the storage layer opens `acs.db`.  Any migration error is fatal: the
daemon logs it and exits rather than running against a partially-migrated
database.

```rust
// acs/src/migrations/mod.rs — the entire ACS-side configuration:
Runner::new(data_dir.join("acs.db"))
    .migrations(registry())                    // ACS's Vec<Box<dyn Migration>>
    .schema_probe(|tx| tx.table_exists("workflows"))
    .run()
```

**Every migration is a Rust file** (`acs/src/migrations/mNNN_<name>.rs`)
implementing the framework's `Migration` trait:

```rust
pub trait Migration: Send + Sync {
    fn name(&self) -> &'static str;      // stable; PK in schema_migrations
    fn baseline(&self) -> bool { false } // baseline convention hook
    fn rebuild(&self) -> bool { false }  // PRAGMA rebuild convention hook
    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError>;
}
```

The framework hands `up()` a `MigrationTx` — a small SQL-string API over the
runner-owned transaction (no ORM, no derive machinery):

* `execute_batch(sql)` — run one or more `;`-separated statements from a SQL
  string constant.  No `BEGIN`/`COMMIT`; the runner owns the transaction.
* `execute(sql, &[SqlValue])` — one statement with positional parameters;
  returns rows affected.
* `query(sql, &[SqlValue]) -> Vec<Vec<SqlValue>>` — read query output back
  as plain Rust values (`SqlValue`: `Null` / `Integer` / `Real` / `Text` /
  `Blob`, with `as_str()` / `as_i64()` / `as_f64()` / `as_blob()` accessors).

Simple migrations keep their SQL in a string constant and are a single
`execute_batch` call.  Complex migrations mix SQL strings with Rust-level
logic — querying rows, transforming them in Rust (shell tokenization,
recursion through nested step JSON), and writing back via parameterised SQL
— in ways plain SQL cannot express (m005 and m006 do exactly this).

Migrations are **frozen by construction**: SQL constants never change after
they ship, and the Rust logic operates purely on `serde_json::Value` — never
on the live model structs — so changes to the runtime models can never
require editing an already-shipped migration.

### Tracking table: `schema_migrations`

Migration execution is tracked by name in a `schema_migrations` table inside
`{data_dir}/acs.db`.  The table is created by the **runner itself** — not by
a migration — before any migration logic runs:

```sql
CREATE TABLE schema_migrations (
    name        TEXT PRIMARY KEY,             -- stable migration name, e.g. "m008_add_workflow_deleted"
    applied_at  TEXT NOT NULL,                -- RFC 3339 timestamp of the recording
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,                      -- wall-clock run time; NULL for seeded rows
    error       TEXT                          -- error text for failed rows; NULL otherwise
);
```

Migration status is **not** exposed through `/health`.  The table is the only
structured surface; per-migration outcomes are also written to `daemon.log`
via `tracing` lines (e.g. `migration 'm007_add_token_columns' applied in 3ms`
and the startup summary `Migrations applied: [...]`).

### Runner behaviour

`run_pending()` walks the registry in name order.  The tracking table — and
only the tracking table — decides what executes:

| Row state for a migration | Action |
|---|---|
| No row | Run it inside its own transaction, then record `success` or `failed` |
| `status = 'success'` | Skip without executing |
| `status = 'failed'` | **Abort daemon startup** before anything runs, naming the migration and the exact recovery statement |

Failed rows are detected up front: if ANY registry migration has a `failed`
row, the runner records nothing and aborts immediately — even migrations
earlier in the order that have no row yet do not run.

Each migration executes inside its own transaction.  On failure the
migration is **rolled back completely**, its row is recorded with status
`failed` plus the error text, and the runner aborts: later migrations never
run and get no row.

Rows recorded for names the registry does not know — `m001_jobs_to_workflows`
and `m002_json_to_sqlite` on databases upgraded from v4 — are tolerated: they
are reported at info level and left in place, never an error.

**Recovery workflow for a failed migration** (this is the sanctioned
workflow): fix the underlying issue, then delete the tracking row so the next
startup re-runs it:

```sql
DELETE FROM schema_migrations WHERE name = '<migration name>';
```

**Caution:** unlike the pre-v5 Rust migrations, v5 migrations are not
internally idempotent — `m004_drop_input_schema` re-run against a migrated
schema fails, for example.  Delete a row (failed *or* success) only when the
migration's preconditions have been restored; the tracking table, not
in-migration guards, is what prevents double execution.

### Baseline convention (fresh install vs. upgrade)

`m000_baseline` is the fresh-install starting point: it creates the
pre-m003-era schema (inline `UNIQUE` on `workflows.name`, `input_schema`
column present, no token columns, no `deleted` column) so that m003–m008
apply on top and reproduce exactly the schema history an upgraded database
went through.

The baseline opts in via the trait's `baseline()` hook, which changes one
thing: on a database that **already has both migration tracking and a
schema** (the `schema_migrations` table pre-exists and ACS's configured
schema probe finds the `workflows` table — true of every v4.2.14 install),
the runner records a `success` row for the baseline WITHOUT executing it.  The baseline therefore never runs against an
existing schema, and a v4.2.14 database upgrading to v5 executes
**nothing**: baseline seeded, m003–m008 skipped via their recorded rows.

If the tracking table exists but the schema does not — a previous startup
died after the runner created the tracking table but before the baseline
ever executed — the baseline runs normally instead of being seeded, so the
crash window cannot wedge the database.

A database that has a schema but **no** tracking table predates migration
tracking; the framework rejects it before creating or running anything,
with its default guidance ("… has a schema but no schema_migrations
tracking table. Refusing to run migrations against it: seed
schema_migrations rows for the migrations that are already applied, or
start from a database with recorded history.").  A database that has a
schema and a tracking table with **zero recorded rows** (lost history) is
likewise rejected ("… has a schema and a schema_migrations table, but no
recorded migration history. Refusing to run migrations against it: …")
instead of seeding the baseline and then failing on migrations built for
the fresh-install chain.

### Rebuild convention (`PRAGMA foreign_keys`)

`PRAGMA foreign_keys` is a no-op inside a transaction, so a migration that
rebuilds a table participating in a foreign key (drop + rename of a parent
table) cannot toggle enforcement itself.  A migration that opts in via the
trait's `rebuild()` hook gets this treatment from the runner instead:

1. `PRAGMA foreign_keys = OFF` before the transaction opens;
2. the migration body runs inside the transaction;
3. `PRAGMA foreign_key_check` runs inside the transaction before commit —
   any violation fails (and therefore rolls back) the migration;
4. `PRAGMA foreign_keys = ON` is restored after commit or rollback.

`m008_add_workflow_deleted` is the concrete case: it rebuilds `workflows`
(the parent of `workflow_runs.workflow_id`) to drop the inline `UNIQUE`.

### Upgrade paths

| Installed version | Path to v5 |
|---|---|
| Fresh install | Baseline + m003–m008 run in order; all recorded `success` |
| v4.2.14 | Direct.  The runner executes nothing: baseline seeded, m003–m008 skip via recorded rows, m001/m002 rows tolerated |
| Pre-tracking databases | Not supported for direct upgrade: the framework rejects a schema without recorded migration history before running anything |

### Adding a migration

1. Create `acs/src/migrations/mNNN_<name>.rs` (increment NNN past the last
   entry) with a unit struct implementing `milepost::Migration`.  Keep the
   SQL in string constants; the body is usually a single
   `tx.execute_batch(SQL)`.  No `BEGIN`/`COMMIT` — the runner owns the
   transaction.
2. Declare the module and append `Box::new(mNNN_<name>::YourStruct)` to
   `registry()` in `acs/src/migrations/mod.rs` — names must stay in
   ascending order.
3. Add Rust-level logic (via `query` / `execute`) only where SQL alone
   cannot express the transform, documenting why.

---

## 10. Migration history

### m000_baseline (v5.0.0)

A single SQL constant; opts into the baseline convention (`baseline()`
returns `true`).  Creates the pre-m003-era schema on fresh installs:
`workflows` (with inline `UNIQUE` on `name` and the `input_schema` column,
without `deleted`), `workflow_runs` (without the token columns), the three
`workflow_runs` indexes, and `meta`.  Never executes on a database that
already has migration tracking (see the baseline convention above).

### m001_jobs_to_workflows / m002_json_to_sqlite (retired)

The v4-era Rust migrations that converted the pre-ACS-18 `jobs.json` format
to workflows and moved JSON storage into SQLite were retired in v5.0.0,
along with the `migrations.json` backfill.  Databases that still need them
are not supported for direct upgrade (the framework rejects a schema
without recorded migration history).  Their `schema_migrations` rows are
tolerated and left in place on upgraded databases.

### m003_drop_step_output_summary

A single SQL constant (json1).  Strips the legacy `output_summary` key from
every persisted
`StepRun` record in `workflow_runs.steps_json` — per-step output lives in
`{data_dir}/logs/{workflow_id}/{run_id}.log`, framed by each `StepRun`'s
byte offsets, so the inline copy was redundant.  The `UPDATE` rebuilds each
`steps_json` array element-by-element with `json_each` + `json_remove`,
consulting the `json_each.type` column so every element kind round-trips
exactly; only rows where at least one object element actually carries the
key are rewritten.

### m004_drop_input_schema

A single SQL constant: `ALTER TABLE workflows DROP COLUMN input_schema;` —
the column is no
longer carried on `Workflow` / `NewWorkflow` / `WorkflowUpdate`.  The column
is guaranteed present when this runs: fresh installs get it from the
baseline, and upgraded databases skip via the recorded row.

### m005_shell_claude_to_agent

**SQL strings + Rust logic** — the rewrite needs a shell tokenizer
(double/single quotes, backslash escapes), `-p` prompt extraction across
three flag syntaxes, residual-flag template reconstruction, and recursion
into arbitrarily-nested `match` step branches, none of which SQL alone can
express.  The body queries `workflows` through `MigrationTx`, transforms the
step JSON in Rust, and writes back via parameterised SQL.

Rewrites legacy `shell` steps that wrap a
`claude -p ... --output-format stream-json` invocation as proper `agent`
steps of type `claude_code_cli`, so the streaming NDJSON cost parser
captures `cost_usd`.  A step is rewritten when `kind == "shell"`, the
command starts with the literal token `claude`, and it contains
`--output-format stream-json`.  The prompt comes from the first `-p` flag
(`-p "..."`, `-p '...'`, or `-p=...`; escape sequences preserved verbatim).
If the residual flags exactly match the default `claude_code_cli` tail the
step inherits the default template; otherwise a full `command_template` is
emitted so custom flags survive.  Commands matching the criterion without a
`-p` flag (stdin-fed prompt) are left unchanged with a warning.  Rewritten
workflows get `version + 1` and a fresh `updated_at`.

### m006_agent_step_normalize

**SQL strings + Rust logic** — same rationale and shape as m005 (tokenizer +
nested-JSON recursion).

Normalizes `agent` steps that still carry a legacy `command_template` string
into the structured shape: `--model <value>` / `--model=<value>` becomes the
`model` field, the canonical baseline tokens (`claude`, `-p` + value,
`--output-format stream-json`, `--verbose`,
`--dangerously-skip-permissions`) are stripped, whatever remains becomes
`extra_args` (defaulting to `[]`), and `command_template` is removed.
Recurses into `match` branches.  Templates whose first token is not
`claude`, or whose quoting is malformed, are left unchanged with a warning.

### m007_add_token_columns

A single SQL constant.  Adds `total_input_tokens` / `total_output_tokens`
(`INTEGER NOT NULL DEFAULT 0`) to `workflow_runs`; historical rows backfill
to `0` ("tokens not tracked").

### m008_add_workflow_deleted

A SQL constant behind an already-applied guard (if the `deleted` column
already exists — the tracking row was lost — it fails loud with the exact
restore-the-row statement instead of silently resetting soft-delete state);
opts into the rebuild convention (`rebuild()` returns `true`, so the runner
applies the `PRAGMA foreign_keys` treatment above).
Adds the `deleted` soft-delete column to
`workflows` and replaces the inline `UNIQUE` on `name` — which SQLite cannot
drop in place — with a partial unique index over live rows:

```sql
CREATE UNIQUE INDEX idx_workflows_name_live ON workflows(name) WHERE deleted = 0;
```

The SQL creates `workflows_new` in the final shape, copies all rows
(`deleted` defaults to `0`), drops the old table, renames, and creates the
index; the runner's `foreign_key_check` guards against orphaning
`workflow_runs` rows.  `DELETE /api/workflows/{id}` sets `deleted = 1`,
keeping the row (name stored verbatim) and every `workflow_runs` record so
cost/token history survives; the partial index makes the name immediately
reusable by a new live workflow.

---


## 11. Daemon Log Management (`SizeManagedWriter`)

**Source:** `acs/src/daemon/mod.rs`

The daemon process log (`daemon.log`) is managed by a custom writer that
prevents unbounded growth.

### Constants

```rust
const DAEMON_LOG_MAX_BYTES: u64 = 1_073_741_824; // 1 GB
```

### Behaviour

- Opens `daemon.log` in **create + append** mode.
- Seeds `bytes_written` from the current file size.
- On every `write()` call, increments `bytes_written`.
- When `bytes_written >= max_size`, triggers `truncate_oldest_quarter()`.

### Truncation algorithm

1. Reads the entire file content into memory.
2. Calculates the 25% byte offset (`content.len() / 4`).
3. Advances from that offset to the next newline so no line is cut in half.
4. Writes the retained 75% to `daemon.log.tmp`.
5. Renames the temporary file over `daemon.log` (atomic replace).
6. Reopens the file in append mode; resets `bytes_written` to the retained
   size.

If the file is empty, `bytes_written` resets to zero with no I/O.  If no
newline is found after the 25% mark, the entire content is kept.  If the cut
point falls at or beyond the end of the content, the file is truncated to zero
via a truncate-then-reopen without using the temporary file.

On daemon startup, `daemon.log` is **truncated to zero** so each daemon session
starts with a fresh log.
