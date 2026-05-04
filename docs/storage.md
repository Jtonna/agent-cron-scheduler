# Storage and Data Management

This document describes how the Agent Cron Scheduler (ACS) persists workflows,
run records, logs, daemon state, and configuration on disk after the ACS-18
refactor.  All paths below are relative to the **data directory** (`{data_dir}`).

For how the data directory is resolved (CLI flags, env vars, platform defaults),
see [Configuration](configuration.md#data-directory-locations).

---

## 1. Overview

Storage is organised around two complementary concerns:

* **Trait-based stores** — all persistence goes through an `async_trait`
  interface.  Each trait has one filesystem implementation (`Fs*`) and can be
  replaced by an in-memory mock for tests.
* **Single rooted layout** — every file the daemon writes lives under a single
  `data_dir` chosen at startup.  There is no global state outside that tree.

The three active store traits after ACS-18 are:

| Trait | Impl | What it stores |
|---|---|---|
| `WorkflowStore` | `FsWorkflowStore` | Workflow definitions (`workflows.json`) |
| `WorkflowRunStore` | `FsWorkflowRunStore` | Per-run `WorkflowRun` records + index |
| *(daemon)* | `SizeManagedWriter` | Daemon process log (`daemon.log`) |

Log output from step execution is written directly by `FileLogSink` (not
through a store trait).  Migration state is maintained in a standalone
`migrations.json` file managed by the migration runner.

---

## 2. Data Directory Layout

```
{data_dir}/
├── agentcronsystem.pid         # Daemon PID file (exclusive creation prevents duplicate instances)
├── agentcronsystem.port        # TCP port the daemon is listening on
├── config.json                 # Daemon config (fallback location; see configuration.md)
├── daemon.log                  # Daemon process log (size-managed, max 1 GB)
├── workflows.json              # Authoritative list of all workflow definitions
├── migrations.json             # Applied-migration state for the numbered migration runner
├── jobs.json.migrated.<ts>     # Backup of legacy jobs.json after m001 runs (unix timestamp suffix)
├── scripts/                    # Reserved directory (created on startup; not currently used)
├── logs/
│   └── {workflow_id}/          # One directory per workflow, named by UUID
│       └── {run_id}.log        # Combined step output for a single run (append-only)
└── runs/
    ├── index.json              # Map of run_id → workflow_id for O(1) lookup
    └── {workflow_id}/          # One directory per workflow, named by UUID
        └── {run_id}.json       # Full WorkflowRun record (pretty-printed JSON)
```

On daemon startup, `create_data_dirs()` ensures the top-level directory and
the `logs/` and `scripts/` subdirectories exist.  The `runs/` directory is
created by `FsWorkflowRunStore::new()`.

---

## 3. Atomic Writes Pattern

All three file-based stores use the same write strategy to prevent partial
writes from leaving the data directory in a corrupt state:

1. Serialize the new content.
2. Write to a sibling `.tmp` file (e.g. `workflows.json.tmp`).
3. Rename the `.tmp` file over the target file.

On POSIX systems the rename is atomic.  On Windows, `tokio::fs::rename`
performs a non-atomic replace, but this is still safer than writing in-place
because the old file is only replaced after the new content is fully written.
After a successful rename, no `.tmp` file remains on disk.

The files that use this pattern:

| Target | Temporary |
|---|---|
| `workflows.json` | `workflows.json.tmp` |
| `runs/{workflow_id}/{run_id}.json` | `runs/{workflow_id}/{run_id}.json.tmp` |
| `runs/index.json` | `runs/index.json.tmp` |
| `migrations.json` | `migrations.json.tmp` |

---

## 4. Corruption Handling

### workflows.json

When `FsWorkflowStore::new()` reads `workflows.json` and encounters invalid
JSON:

1. The corrupted file is copied to `workflows.json.bak.<unix_timestamp>` (e.g.
   `workflows.json.bak.1746288000`).
2. A warning is logged via `tracing::warn!`.
3. The store starts with an **empty** workflow list.

The timestamped suffix means multiple corruption events produce distinct
backups rather than overwriting each other.

### runs/index.json

When `FsWorkflowRunStore::new()` reads `runs/index.json` and encounters
invalid JSON:

1. The corrupted index is copied to `runs/index.json.bak.<unix_timestamp>`.
2. A warning is logged.
3. The runner **rebuilds** the index by scanning all
   `runs/{workflow_id}/{run_id}.json` files on disk.  Only files whose parent
   directory name is a valid UUID and whose filename stem is a valid UUID are
   included.

This means run history is never lost from a corrupt index; the index is always
recoverable from the run files themselves.

### migrations.json

If `migrations.json` is missing or contains invalid JSON, `read_state()`
returns an empty `HashSet`, treating the daemon as if no migrations have been
applied.  **No backup file is created** (unlike `workflows.json` and
`runs/index.json`); a corrupt state simply causes pending migrations to re-run
(each migration's `run()` method must be idempotent).

---

## 5. WorkflowStore

**Sources:** `acs/src/storage/mod.rs`, `acs/src/storage/workflows.rs`

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
}
```

| Method | Description |
|---|---|
| `list_workflows` | Returns all workflows. |
| `get_workflow` | Looks up a single workflow by UUID; returns `None` if not found. |
| `find_by_name` | Looks up a single workflow by name; returns `None` if not found. |
| `create_workflow` | Validates, assigns a UUIDv7 ID, sets `version: 1`, persists, and returns the new workflow. |
| `update_workflow` | Partial update; bumps `version` when any definition-affecting field changes. Returns `NotFound` or `Conflict` as appropriate. |
| `delete_workflow` | Removes a workflow by UUID; returns `NotFound` if it does not exist. |

### FsWorkflowStore

```rust
pub struct FsWorkflowStore {
    path: PathBuf,           // {data_dir}/workflows.json
    inner: RwLock<Vec<Workflow>>,
}
```

All workflow data is held in a `tokio::sync::RwLock<Vec<Workflow>>`.  Reads
acquire a read lock; mutations acquire a write lock.  After every mutation the
full list is persisted to `workflows.json` via `persist()`.

### On-disk format

`workflows.json` is a pretty-printed JSON array of `Workflow` objects
(`serde_json::to_string_pretty`).  The array may be empty (`[]`).

### Version bump rules

`update_workflow` tracks whether any **definition-affecting field** changed.
Definition-affecting fields are: `steps`, `on_failure`, `input_schema`,
`default_input`, `working_dir`, `env_vars`, `allow_concurrent`, `schedule`,
`schedule_mode`, `timezone`, and `name`.

The `enabled` flag is explicitly excluded — toggling a workflow on or off does
not alter its definition and therefore does **not** bump `version`.

Runtime metadata fields (`last_run_at`, `last_run_status`, `last_run_id`) are
not present in `WorkflowUpdate` at all and cannot trigger a version bump.

### Duplicate name enforcement

Both `create_workflow` and `update_workflow` reject duplicate names among
existing workflows, returning `AcsError::Conflict`.

---

## 6. WorkflowRunStore

**Source:** `acs/src/storage/workflow_runs.rs`

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
}
```

| Method | Description |
|---|---|
| `create_run` | Writes the initial run file and updates the index. |
| `update_run` | Atomically replaces the run file (uses index to locate the workflow directory). |
| `get_run` | Uses index for O(1) lookup; reads and deserializes the run file. Returns `None` if not in index or file is absent. |
| `list_runs` | Lists runs for a workflow, latest-first. `limit=0` returns all. Supports `offset` for pagination. Skips corrupted files with a warning. |
| `count_runs` | Returns the number of run files in a workflow's directory. |
| `delete_run` | Removes the run from the index (persisted atomically) and deletes the `.json` file. Also attempts to delete the matching log file at `logs/{workflow_id}/{run_id}.log` (best-effort; logs a warning if the file is absent or cannot be removed). |

### FsWorkflowRunStore

```rust
pub struct FsWorkflowRunStore {
    runs_dir: PathBuf,           // {data_dir}/runs/
    index: Arc<Mutex<RunIndex>>, // run_id → workflow_id (in-memory + on-disk)
}
```

### Persistence paths

Each run is stored as a pretty-printed JSON file:

```
{data_dir}/runs/{workflow_id}/{run_id}.json
```

The trigger handler persists the initial `Running` record synchronously before
spawning the workflow executor, so `GET /api/runs/{id}` between trigger and
execution start always returns the run rather than a 404.

### Index file design

`{data_dir}/runs/index.json` contains a flat JSON object mapping every known
`run_id` (UUID string) to its `workflow_id` (UUID string):

```json
{
  "019abcde-1234-7000-8000-aabbccddeeff": "01912345-6789-7abc-def0-123456789abc"
}
```

This enables O(1) `get_run` lookups without scanning workflow subdirectories.
The index is kept in sync with the in-memory `RunIndex` cache and written
atomically after every `create_run` or `delete_run` operation.

### Latest-first ordering

`list_runs` sorts run IDs descending before applying `offset`/`limit`.  Run
IDs are UUIDv7 values, which are monotonically time-ordered, so descending UUID
order is equivalent to latest-first chronological order without reading any
file contents.

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
there was nothing to do.

### State file

Applied migration names are tracked in `{data_dir}/migrations.json`:

```json
{
  "applied": ["m001_jobs_to_workflows"]
}
```

`run_pending()` reads this file at daemon startup, skips already-applied
migrations, and writes the updated state after each successful migration.
Writes are atomic (`.tmp` + rename).

If the file is missing, `read_state()` returns an empty set (fresh install).
If the file is corrupt, `read_state()` logs a warning and returns an empty set
(safe to re-run migrations; each is idempotent).

### Runner behaviour

`run_pending()` iterates the registry in order.  On the first migration error
it stops and propagates the error; partial progress (applied migrations before
the failure) is preserved in the state file.  Migrations that return
`Ok(false)` (nothing to do) are recorded in `skipped_not_needed` but are
**not** added to the applied set.

### Adding a migration

1. Create `acs/src/migration/mNNN_<name>.rs` (increment NNN).
2. Implement `Migration` for a unit struct.
3. Append `Box::new(mNNN_<name>::YourStruct)` to the `registry()` function in
   `mod.rs`.

---

## 10. Backwards Compatibility — jobs.json

Migration `m001_jobs_to_workflows` handles the transition from the pre-ACS-18
`jobs.json` format.

### Decision table

| Condition | Action |
|---|---|
| `workflows.json` already exists | No-op (return `Ok(false)`) |
| `jobs.json` does not exist | No-op — fresh install (return `Ok(false)`) |
| Both conditions false | Read `jobs.json`, synthesise workflows, write `workflows.json`, rename `jobs.json` |

### Backup file name

After a successful migration, the original `jobs.json` is **renamed** (not
deleted) to:

```
{data_dir}/jobs.json.migrated.<unix_timestamp>
```

For example: `jobs.json.migrated.1746288000`.  The file is never deleted
automatically.  Multiple failed-then-retried migrations cannot produce duplicate
backup names because the presence of `workflows.json` causes the migration to
short-circuit as a no-op after the first successful run.

### Step synthesis rules

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

When a non-shell hook body is written to a script file, the `migrated_scripts/` directory is created under `{data_dir}` during the migration run. The script file content is identical to the original hook body from `jobs.json`.

`job.timeout_secs` is copied to `common.timeout_secs` on all synthesised
steps.  A value of `0` becomes `None` (no timeout).

`job.last_exit_code` is mapped to `workflow.last_run_status`: `0` →
`Completed`, any other value → `Failed`, absent → `None`.

`job.allow_concurrent` is preserved verbatim.  The new-workflow default is
`true`, but migrated entries keep whatever value the original job had.

`script_type` is inferred from the file extension: `.sh`/`.bash` → `"shell"`,
`.bat`/`.cmd` → `"batch"`, `.py` → `"python"`, `.ps1` → `"powershell"`.
Unrecognised extensions → `None`.

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
