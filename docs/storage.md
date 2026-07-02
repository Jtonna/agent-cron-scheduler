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
(the standalone `migrations.json` file is legacy — see
[Migration System](#9-migration-system)).

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
├── migrations.json             # Legacy migration state; read once to backfill schema_migrations, never written again
├── jobs.json.migrated.<ts>     # Backup of legacy jobs.json after m001 runs (unix timestamp suffix)
├── migrated_scripts/           # Created on demand by m001 migration when migrating non-shell hooks
├── scripts/                    # Reserved directory (created on startup; not currently used)
└── logs/
    └── {workflow_id}/          # One directory per workflow, named by UUID
        └── {run_id}.log        # Combined step output for a single run (append-only)
```

On daemon startup, `create_data_dirs()` ensures the top-level directory and
the `logs/` and `scripts/` subdirectories exist.  The `acs.db` file is created
by the migration runner (to host its `schema_migrations` tracking table)
before any migration runs; the `m002_json_to_sqlite` migration then applies
the full data schema when the daemon first runs.  The `acs.db-wal` and
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
tables.  The legacy `migrations.json` file is read exactly once — to backfill
the tracking table on databases that predate it.  If it is missing or
contains invalid JSON at that moment, the backfill seeds nothing and every
migration runs normally; each migration's internal idempotency makes that
safe on an already-migrated database.

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

The cost-analytics aggregation query — `SUM(CASE WHEN finished_at >= ? THEN total_cost_usd END)` plus `COUNT(...)` over two windows — runs once per workflow per `GET /api/cost/workflows[/{id}]` cache miss. The `idx_workflow_runs_workflow_id_finished_at` composite index established in m002 covers this access pattern: the WHERE clause filters by `workflow_id`, `status IN (...)`, and `finished_at >= ?`, with the conditional SUMs evaluated over the filtered rows.

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

**Sources:** `acs/src/migration/mod.rs`,
`acs/src/migration/m001_jobs_to_workflows.rs`,
`acs/src/migration/m002_json_to_sqlite.rs`,
`acs/src/migration/m003_drop_step_output_summary.rs`,
`acs/src/migration/m004_drop_input_schema.rs`,
`acs/src/migration/m005_shell_claude_to_agent.rs`,
`acs/src/migration/m006_agent_step_normalize.rs`,
`acs/src/migration/m007_add_token_columns.rs`,
`acs/src/migration/m008_add_workflow_deleted.rs`,
`acs/src/migration/legacy_types.rs`

### Design

Migrations are numbered files: `mNNN_<name>.rs` (the `m` prefix is required
because Rust module names cannot start with a digit).  Each file contains a
unit struct implementing the `Migration` trait:

```rust
#[async_trait]
pub trait Migration: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError>;
}
```

`run()` must be **idempotent** — re-running on already-migrated data must be a
no-op.  It returns `Ok(true)` when work was performed, or `Ok(false)` when
there was nothing to do.  The runner records `success` either way; the return
value only affects logging.  The internal idempotency checks are a safety
property (they make the delete-row-and-re-run workflow below harmless) — the
runner never consults them to decide execution.

### Tracking table: `schema_migrations`

Migration execution is tracked by name in a `schema_migrations` table inside
`{data_dir}/acs.db`.  The table is created by the **runner itself** — not by a
migration — before any migration logic runs:

```sql
CREATE TABLE schema_migrations (
    name        TEXT PRIMARY KEY,             -- stable migration name, e.g. "m008_add_workflow_deleted"
    applied_at  TEXT NOT NULL,                -- RFC 3339 timestamp of the recording
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,                      -- wall-clock run time; NULL for backfilled rows
    error       TEXT                          -- error text for failed rows; NULL otherwise
);
```

Migration status is **not** exposed through `/health`.  The table is the only
structured surface; per-migration outcomes are also written to `daemon.log`
via `tracing::info!` lines (e.g.
`"Migration m003 complete: stripped output_summary from N workflow_runs row(s)"`).

### Runner behaviour

`run_pending()` iterates the registry in order.  The tracking table — and only
the tracking table — decides what executes:

| Row state for a migration | Action |
|---|---|
| No row | Run it, then record `success` or `failed` |
| `status = 'success'` | Skip without invoking the migration |
| `status = 'failed'` | **Abort daemon startup** with an error naming the migration |

A migration that runs but finds nothing to do (e.g. legacy format absent)
still records `success` — the row means "processed", so it never re-runs.  On
failure the row is recorded with status `failed` and the error text, and the
runner aborts immediately: later migrations do not run and get no row.

**Recovery workflow for a failed migration** (this is the sanctioned dev
workflow): fix the underlying issue, then delete the tracking row so the next
startup re-runs it:

```sql
DELETE FROM schema_migrations WHERE name = '<migration name>';
```

The same mechanism re-runs any migration whose `success` row is deleted —
safe, because every migration is internally idempotent.

### Backfill from legacy `migrations.json`

Databases that predate the tracking table recorded applied migrations in
`{data_dir}/migrations.json`:

```json
{ "applied": ["m001_jobs_to_workflows", "m002_json_to_sqlite"] }
```

The first time the runner creates the tracking table it seeds a `success` row
for every registry migration named in that file — those are treated as
already applied without being invoked.  Every other migration then runs
normally; on an already-migrated database their internal idempotency makes
that a safe no-op, recorded as `success`.  After this one-time backfill the
table is the sole source of truth: `migrations.json` is left on disk but
never written (or read) again.  A fresh install has no legacy file, so all
migrations simply run once and end recorded as `success`.

### Adding a migration

1. Create `acs/src/migration/mNNN_<name>.rs` (increment NNN).
2. Implement `Migration` for a unit struct.
3. Append `Box::new(mNNN_<name>::YourStruct)` to the `registry()` function in
   `mod.rs`.

---

## 10. Migration history

### m001_jobs_to_workflows

`m001_jobs_to_workflows` handles the transition from the pre-ACS-18
`jobs.json` format.

| Condition | Action |
|---|---|
| `workflows.json` already exists | No-op (return `Ok(false)`) |
| `jobs.json` does not exist | No-op — fresh install (return `Ok(false)`) |
| Both conditions false | Read `jobs.json`, synthesise workflows, write `workflows.json`, rename `jobs.json` |

After a successful migration, the original `jobs.json` is **renamed** (not
deleted) to `{data_dir}/jobs.json.migrated.<unix_timestamp>`.

For each legacy `Job`, the synthesised `Workflow` preserves the original job's
UUID so that existing log files (keyed by `job_id` / `workflow_id`) remain
accessible without path changes.

| Legacy field | Synthesised step |
|---|---|
| `pre_hook` (if present) with `pre_hook_script_type = null` or `"shell"` | `ShellStep` with `id="pre_hook"`, `on_failure=Abort`, `always_run=false` |
| `pre_hook` (if present) with `pre_hook_script_type = "python"`, `"batch"`, or `"powershell"` | Hook body written to `migrated_scripts/{job_id}_pre_hook.{ext}`; `ScriptStep` with `id="pre_hook"`, `script_type` set, `always_run=false` |
| `execution: ShellCommand(cmd)` | `ShellStep` with `id="main"`, `on_failure=Abort` |
| `execution: ScriptFile(path)` | `ScriptStep` with `id="main"`, `script_type` inferred from extension |
| `post_hook` (if present) with `post_hook_script_type = null` or `"shell"` | `ShellStep` with `id="post_hook"`, `on_failure=Abort`, `always_run=true` |
| `post_hook` (if present) with `post_hook_script_type = "python"`, `"batch"`, or `"powershell"` | Hook body written to `migrated_scripts/{job_id}_post_hook.{ext}`; `ScriptStep` with `id="post_hook"`, `script_type` set, `always_run=true` |

`job.timeout_secs` is copied to `common.timeout_secs` on all synthesised
steps.  A value of `0` becomes `None` (no timeout).  `job.last_exit_code` is
mapped to `workflow.last_run_status`: `0` → `Completed`, any other value →
`Failed`, absent → `None`.  `job.allow_concurrent` is preserved verbatim.
`script_type` is inferred from the file extension: `.sh`/`.bash` → `"shell"`,
`.bat`/`.cmd` → `"batch"`, `.py` → `"python"`, `.ps1` → `"powershell"`.
Unrecognised extensions → `None`.

### m002_json_to_sqlite

`m002_json_to_sqlite` populates `acs.db` from any JSON sources left behind by
earlier daemon versions.

| Condition | Action |
|---|---|
| `acs.db` contains a `workflows` table | No-op (return `Ok(false)`) — conversion or fresh-install schema already in place. Table-based rather than file-based because the migration runner creates `acs.db` up front to host `schema_migrations`. |
| Neither `workflows.json` nor `runs/` exists | Apply the schema to `acs.db`, return `Ok(true)` |
| Otherwise | Open `acs.db`, BEGIN transaction, INSERT every workflow from `workflows.json` and every run from `runs/<workflow_id>/<run_id>.json`, verify row counts match, sample-verify one row of each, COMMIT, then delete `workflows.json` and the entire `runs/` directory |

Files inside `runs/` whose name is not a valid UUID `.json` filename (notably
`runs/index.json`) are ignored on read; they are removed implicitly when the
parent `runs/` directory is deleted on success.

On any failure during the insert/verify phase the transaction is rolled back
explicitly, the partial `acs.db` (and any WAL/SHM sidecars) is removed, and
the JSON sources are left untouched so the migration can be re-run after the
underlying problem is fixed.  The error is propagated through
`migration::run_pending()`, which aborts daemon startup.

`migrations.json` is **not** moved into the `meta` table.  It remains on disk
as the legacy migration-state file: the runner reads it once to backfill the
`schema_migrations` tracking table and never touches it again.

### m003_drop_step_output_summary

`m003_drop_step_output_summary` walks every row in `workflow_runs` and strips
the legacy `output_summary` key from each persisted `StepRun` JSON record.
Per-step output now lives exclusively in
`{data_dir}/logs/{workflow_id}/{run_id}.log`, framed by each `StepRun`'s
`log_byte_offset_start` / `log_byte_offset_end` pair, so the inline copy is
redundant.

| Condition | Action |
|---|---|
| `acs.db` does not exist | No-op (return `Ok(false)`) |
| No row in `workflow_runs` carries an `output_summary` key | No-op (return `Ok(false)`) — drops the read-only transaction without committing |
| Otherwise | BEGIN transaction → for each row whose `steps_json` contains at least one `output_summary` key: parse, strip, re-serialise, `UPDATE workflow_runs SET steps_json = ? WHERE run_id = ?` → COMMIT |

On any parse / serialise / UPDATE error the transaction is rolled back and the
error is propagated through `run_pending()`, which aborts daemon startup. The
migration is idempotent: re-running on a database that already has no
`output_summary` keys returns `Ok(false)` and the runner does not write it
into the applied set.

### m004_drop_input_schema

`m004_drop_input_schema` removes the `input_schema` column from the
`workflows` table. `input_schema` is no longer carried on `Workflow`,
`NewWorkflow`, or `WorkflowUpdate` and is not consumed by the runtime; the
column must be dropped so that INSERT/UPDATE statements that no longer
reference it succeed.

| Condition | Action |
|---|---|
| `acs.db` does not exist | No-op (return `Ok(false)`) |
| `workflows.input_schema` column is absent (fresh install or already migrated) | No-op (return `Ok(false)`) |
| Column is present | `ALTER TABLE workflows DROP COLUMN input_schema` |

Column existence is checked via `PRAGMA table_info(workflows)`. The migration
is idempotent: a second run sees the column is absent and returns `Ok(false)`.

### m005_shell_claude_to_agent

`m005_shell_claude_to_agent` rewrites legacy `shell` steps that wrap a
`claude -p ... --output-format stream-json` invocation as proper `agent` steps
of type `claude_code_cli`, so that the streaming NDJSON cost parser captures
`cost_usd`. The cost parser only runs on `AgentStep` — shell-wrapped calls
were correct functionally but always produced a `null` `cost_usd`.

A step is rewritten when **all** of the following hold:

* `kind == "shell"`
* The command, trimmed of leading whitespace, starts with the literal token
  `claude` (`"claude"` exactly, `"claude "`, or `"claude\t"`).
* The command contains the substring `--output-format stream-json`.

The prompt is extracted from the first `-p` flag's value. Supported syntaxes
are `-p "double-quoted"`, `-p 'single-quoted'`, `-p=value`, `-p="value"`, and
`-p='value'`. Embedded `\n`, `\t`, escaped quotes, and `\\` sequences inside
the quoted prompt are preserved verbatim (de-quoted without de-escaping).

If the residual flags (after removing `-p <value>`) exactly match
`--output-format stream-json --verbose --dangerously-skip-permissions` —
the default `claude_code_cli` tail — the rewritten step has
`command_template = None` (it inherits the default). Otherwise the residual
flags are preserved by emitting a full `command_template` so callers retain
custom flags like `--model`, `--session-id`, or `-c`.

**Skip case:** if the command matches the detection criterion but has no
`-p` flag, the prompt is being fed via stdin. Automatically migrating that
case would lose the stdin source, so the migration emits a `tracing::warn!`
and leaves the step as `shell`.

| Condition | Action |
|---|---|
| `acs.db` does not exist | No-op (return `Ok(false)`) |
| No workflow contains a shell-claude step matching the criterion | No-op (return `Ok(false)`) |
| Otherwise | BEGIN transaction → for each workflow whose `steps_json` carries at least one rewritten step: walk the step array (including recursion into every `MatchStep` `cases.<value>` array and `default` array), rewrite each shell-claude step in place, bump `workflows.version`, set `updated_at`, UPDATE → COMMIT |

Rollback on error mirrors m003. The migration is idempotent: after rewriting,
the step's `kind` is `agent` and the detection criterion no longer matches.

### m006_agent_step_normalize

`m006_agent_step_normalize` walks every row in `workflows`, finds `agent` steps that
still carry a legacy `command_template` field, and rewrites them into the structured
v4.2.7 shape (`model` + `extra_args`). The `command_template` field was removed in
v4.2.7 because the agent runner now builds argv directly without going through a shell,
eliminating a whole class of escaping bugs.

Detection criteria for rewriting a step:

* `kind == "agent"`
* `command_template` key is present on the step JSON

For each match the migration:

1. Tokenizes `command_template` (handles quoted values, `--flag=value` and `--flag value`
   forms, single/double quotes, escaped quotes).
2. Extracts `--model <value>` (or `--model=<value>`) if present and sets the `model`
   field. If `--model` is absent, any pre-existing `model` value on the step is
   preserved untouched.
3. Strips the canonical baseline tokens: `claude` (first token), `-p <value>`,
   `--output-format stream-json`, `--verbose`, `--dangerously-skip-permissions`.
4. Whatever tokens remain become `extra_args` (an array of strings, defaulting to `[]`).
5. Removes the `command_template` key entirely.
6. Recurses into `MatchStep` `cases.<value>` and `default` arrays so rewrites apply
   throughout branching workflows.

Templates whose first token is not `claude`, or whose quoting is malformed, are left
unchanged with a `tracing::warn!`. Operators can rewrite those by hand.

| Condition | Action |
|---|---|
| `acs.db` does not exist | No-op (return `Ok(false)`) |
| No workflow has an AgentStep with `command_template` | No-op (return `Ok(false)`) |
| Otherwise | BEGIN transaction → for each workflow whose `steps_json` is rewritten: bump `version`, refresh `updated_at`, UPDATE → COMMIT |

Rollback on parse/UPDATE error. Idempotent: after rewriting, no `command_template` keys
remain, so a second run sees no candidates and returns `Ok(false)`.

### m007_add_token_columns

`m007_add_token_columns` adds the `total_input_tokens` and `total_output_tokens`
columns to the `workflow_runs` table introduced in v4.2.11. Both columns are
`INTEGER NOT NULL DEFAULT 0`, so historical rows transparently backfill to `0`
("tokens not tracked"). Fresh installs receive the columns directly via
`schema.rs` and this migration short-circuits.

| Condition | Action |
|---|---|
| `acs.db` does not exist | No-op (return `Ok(false)`) — fresh install |
| `total_input_tokens` column already present on `workflow_runs` | No-op (return `Ok(false)`) — idempotent |
| Otherwise | BEGIN transaction → `ALTER TABLE workflow_runs ADD COLUMN total_input_tokens INTEGER NOT NULL DEFAULT 0;` and `ALTER TABLE workflow_runs ADD COLUMN total_output_tokens INTEGER NOT NULL DEFAULT 0;` → COMMIT |

---

### m008_add_workflow_deleted

`m008_add_workflow_deleted` (v4.2.14) adds the `deleted` soft-delete
column to the `workflows` table and replaces the inline `UNIQUE` constraint on
`name` with a **partial unique index** covering live rows only:

```sql
CREATE UNIQUE INDEX idx_workflows_name_live ON workflows(name) WHERE deleted = 0;
```

SQLite cannot drop an inline column constraint in place, so the migration
performs a standard table rebuild inside a single transaction (with
`PRAGMA foreign_keys = OFF` for its duration, toggled outside the transaction
where the pragma actually takes effect): create `workflows_new` without the
inline `UNIQUE`, copy all rows, drop the old table, rename, create the partial
index, then run `PRAGMA foreign_key_check` and roll back if the rebuild would
orphan any `workflow_runs` rows.

`DELETE /api/workflows/{id}` sets `deleted = 1`, keeping the row (name stored
verbatim) and every `workflow_runs` record so cost/token history survives.
Because the unique index only covers `deleted = 0` rows, the name is
immediately reusable by a new workflow while name resolution among live
workflows stays unambiguous.

Existing workflows backfill to `deleted = 0`. Fresh installs receive the new
table shape and index directly via `schema.rs` and this migration
short-circuits.

| Condition | Action |
|---|---|
| `acs.db` does not exist | No-op (return `Ok(false)`) — fresh install |
| `workflows` DDL no longer contains `UNIQUE` | No-op (return `Ok(false)`) — idempotent (already rebuilt or created fresh) |
| Otherwise | Table rebuild: additively ensure `is_favorited`/`deleted` columns → `CREATE TABLE workflows_new` (no inline `UNIQUE`) → copy rows → `DROP`/`RENAME` → `CREATE UNIQUE INDEX idx_workflows_name_live ...` → `foreign_key_check` → COMMIT |

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
