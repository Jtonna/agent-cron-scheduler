# Storage and Data Management

This document describes how the Agent Cron Scheduler (ACS) persists jobs, run
logs, daemon state, and configuration on disk.  All paths below are relative to
the **data directory** (`{data_dir}`).

---

## 1. Data Directory Layout

```
{data_dir}/
├── acs.pid              # Daemon process ID (exclusive lock file)
├── acs.port             # TCP port the daemon is listening on
├── config.json          # Daemon config (fallback location, priority 4 of 5; see configuration.md)
├── daemon.log           # Daemon process log (size-managed, max 1 GB)
├── jobs.json            # Authoritative list of all registered jobs
├── scripts/             # Reserved directory (created on startup; not currently used for ScriptFile path resolution)
└── logs/
    └── {job_id}/        # One directory per job, named by UUID
        ├── {run_id}.log          # Raw process output for a single run
        └── {run_id}.meta.json    # Structured metadata for a single run
```

For how the data directory is resolved (CLI flags, env vars, platform defaults), see
[Configuration](configuration.md#data-directory-locations).

On daemon startup the function `create_data_dirs()` ensures the top-level
directory and both the `logs/` and `scripts/` subdirectories exist.

---

## 2. Job Storage (`JsonJobStore`)

**Source:** `acs/src/storage/jobs.rs`

### Struct definition

```rust
pub struct JsonJobStore {
    file_path: PathBuf,    // {data_dir}/jobs.json
    cache: RwLock<Vec<Job>>,
}
```

### On-disk format

`jobs.json` contains a pretty-printed JSON array of `Job` objects, serialized
with `serde_json::to_string_pretty`.  Example:

```json
[
  {
    "id": "01912345-6789-7abc-def0-123456789abc",
    "name": "backup-db",
    "schedule": "0 2 * * *",
    "execution": { "type": "ShellCommand", "value": "pg_dump mydb > /backups/db.sql" },
    "enabled": true,
    "timezone": null,
    "working_dir": null,
    "env_vars": null,
    "timeout_secs": 0,
    "log_environment": false,
    "created_at": "2025-06-01T12:00:00Z",
    "updated_at": "2025-06-01T12:00:00Z",
    "last_run_at": null,
    "last_exit_code": null,
    "next_run_at": null
  }
]
```

Note: `next_run_at` is serialized to `jobs.json` but is always `null` on disk. It is skipped during deserialization (`#[serde(skip_deserializing)]`) and only computed at runtime in API response handlers.

### In-memory caching

All job data is held in a `tokio::sync::RwLock<Vec<Job>>`.  Reads acquire a
**read lock**; mutations acquire a **write lock**.  After every mutation the
full list is persisted to disk via `persist()`.

### Thread safety

`JsonJobStore` is `Send + Sync`.  The `RwLock` allows concurrent readers while
serializing writers, making it safe for simultaneous API requests.

### Atomic writes

The `persist` method writes to a temporary file first, then renames it over the
target:

```rust
async fn persist(&self, jobs: &[Job]) -> Result<()> {
    let tmp_path = self.file_path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(jobs)?;
    tokio::fs::write(&tmp_path, json.as_bytes()).await?;
    tokio::fs::rename(&tmp_path, &self.file_path).await?;
    Ok(())
}
```

The temporary file is `jobs.json.tmp`.  After a successful rename no `.tmp`
file remains on disk.

### Corruption recovery

When `JsonJobStore::new()` loads `jobs.json` and encounters invalid JSON:

1. The corrupted file is copied to `jobs.json.bak`.
2. A warning is logged.
3. The store starts with an **empty** job list.

This prevents a single corrupted byte from permanently locking the user out of
the system.

### Duplicate name enforcement

Both `create_job` and `update_job` check for name collisions among existing
jobs, returning an `AcsError::Conflict` if a duplicate is found.

---

## 3. Log Storage (`FsLogStore`)

**Source:** `acs/src/storage/logs.rs`

### Struct definition

```rust
pub struct FsLogStore {
    logs_dir: PathBuf,  // {data_dir}/logs/
}
```

### Directory structure

Each job gets its own subdirectory under `logs/`, named by the job's UUID.
Inside that directory, each run produces two files:

| File | Description |
|---|---|
| `{run_id}.log` | Raw process output (stdout + stderr), appended incrementally |
| `{run_id}.meta.json` | Structured metadata (`JobRun` struct as pretty-printed JSON) |

### Metadata file format (`{run_id}.meta.json`)

```json
{
  "run_id": "019abcde-1234-7000-8000-aabbccddeeff",
  "job_id": "01912345-6789-7abc-def0-123456789abc",
  "started_at": "2025-06-15T02:00:00Z",
  "finished_at": "2025-06-15T02:00:05Z",
  "status": "Completed",
  "exit_code": 0,
  "log_size_bytes": 1024,
  "error": null
}
```

The `status` field is one of: `"Running"`, `"Completed"`, `"Failed"`, or
`"Killed"`.

### Append-mode writing

Log output is written incrementally as the job produces it.  `append_log` opens
the file with `create(true).append(true)` and calls `write_all` followed by
`flush`:

```rust
async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> Result<()> {
    let job_dir = self.job_dir(job_id);
    tokio::fs::create_dir_all(&job_dir)
        .await
        .context("Failed to create job log directory")?;
    let log_path = self.log_path(job_id, run_id);
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .await?;
    file.write_all(data).await?;
    file.flush().await?;
    Ok(())
}
```

### Tail reading

`read_log` supports an optional `tail` parameter.  When provided, only the last
`n` lines of the log file are returned.  When `None`, the entire file content
is returned.  If the log file does not exist, an empty string is returned.

### Run listing and pagination

`list_runs` reads all `.meta.json` files in a job's log directory, sorts them
by `started_at` **descending** (newest first), and applies `offset` and `limit`
for pagination.  It returns both the paginated slice and the **total** count of
runs.

Malformed `.meta.json` files are skipped with a warning rather than causing a
hard error.

---

## 4. Log Rotation

**Source:** `acs/src/storage/logs.rs` -- `FsLogStore::cleanup()`

The maximum number of retained runs per job is controlled by the `max_log_files_per_job`
config field (see [Configuration](configuration.md#field-reference)). Note: `max_log_file_size`
is defined in `DaemonConfig` but is **not currently enforced** at runtime.

### Cleanup behavior

The `cleanup` method is called after a job run completes.  It:

1. Reads all `.meta.json` files in the job's log directory.
2. If the count does not exceed `max_files` (i.e., `<= max_files`), returns immediately (no-op).
3. Sorts runs by `started_at` **ascending** (oldest first).
4. Computes `to_remove = runs.len() - max_files`.
5. For each of the `to_remove` oldest runs, deletes both the `.meta.json` and
   `.log` files.

Malformed `.meta.json` files are skipped with a warning and are not counted
toward the run total, so they will not be cleaned up by this method.

This ensures the most recent runs are always preserved.

If the job's log directory does not exist (the job has never run), cleanup
succeeds silently.

---

## 5. Daemon Log Management (`SizeManagedWriter`)

**Source:** `acs/src/daemon/mod.rs`

The daemon process log (`daemon.log`) is managed by a custom writer that
prevents unbounded growth.

### Constants

```rust
const DAEMON_LOG_MAX_BYTES: u64 = 1_073_741_824; // 1 GB
```

### `SizeManagedWriter` struct

```rust
struct SizeManagedWriter {
    file: std::fs::File,
    path: PathBuf,
    bytes_written: u64,
    max_size: u64,
}
```

### Behavior

- Opens `daemon.log` in **create + append** mode.
- Seeds `bytes_written` from the current file size so that truncation triggers
  correctly even when the file already has content.
- On every `write()` call, increments `bytes_written` by the number of bytes
  written.
- When `bytes_written >= max_size`, triggers `truncate_oldest_quarter()`.

### Truncation algorithm (`truncate_oldest_quarter`)

1. Reads the entire file content into memory.
2. Calculates a 25% byte offset (`content.len() / 4`).
3. Advances from the 25% offset to the **next newline boundary** so that no
   line is cut in half.
4. Writes the retained 75% portion to a temporary file (`daemon.log.tmp`).
5. Renames the temporary file over `daemon.log` (atomic replace).
6. Reopens the file in append mode and resets `bytes_written` to the retained
   size.

If the file is empty, `bytes_written` is reset to zero and no I/O occurs.  If
no newline is found after the 25% mark (degenerate single-line case), the
entire content is kept.

### Startup behavior

On daemon startup, `daemon.log` is **truncated to zero** (via
`std::fs::File::create`) so each daemon session starts with a fresh log.  The
`SizeManagedWriter` is then created and connected to the `tracing` framework
via `tracing_appender::non_blocking`.

---

## 6. Orphaned Log Cleanup

**Source:** `acs/src/daemon/mod.rs` -- `cleanup_orphaned_logs()`

When the daemon starts, it scans the `logs/` directory for subdirectories whose
names are valid UUIDs.  For each such directory, it checks whether a
corresponding job exists in the job store.  If the job has been deleted, the
entire log directory (including all run files) is removed via
`remove_dir_all`.

### Rules

- Only directories whose names parse as valid UUIDs are considered.
- Non-UUID directories (e.g., `not-a-uuid`) are left untouched.
- If the `logs/` directory does not exist, cleanup returns immediately.
- Failures to remove individual orphaned directories are logged as warnings but
  do not abort the cleanup of remaining directories.

---

## 7. Storage Traits

**Source:** `acs/src/storage/mod.rs`

Both storage backends implement async traits, enabling testing with in-memory
mock implementations.

### `JobStore` trait

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

| Method | Description |
|---|---|
| `list_jobs` | Returns all jobs. |
| `get_job` | Looks up a single job by UUID; returns `None` if not found. |
| `find_by_name` | Looks up a single job by name; returns `None` if not found. |
| `create_job` | Validates, assigns a UUIDv7 ID, persists, and returns the new job. |
| `update_job` | Partial update of a job's fields; returns `NotFound` or `Conflict` errors as appropriate. |
| `delete_job` | Removes a job by UUID; returns `NotFound` if the job does not exist. |

### `LogStore` trait

```rust
#[async_trait]
pub trait LogStore: Send + Sync {
    async fn create_run(&self, run: &JobRun) -> Result<()>;
    async fn update_run(&self, run: &JobRun) -> Result<()>;
    async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> Result<()>;
    async fn read_log(&self, job_id: Uuid, run_id: Uuid, tail: Option<usize>) -> Result<String>;
    async fn list_runs(
        &self,
        job_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobRun>, usize)>;
    async fn cleanup(&self, job_id: Uuid, max_files: usize) -> Result<()>;
}
```

| Method | Description |
|---|---|
| `create_run` | Creates the job log directory (if needed) and writes the initial `.meta.json`. |
| `update_run` | Overwrites the `.meta.json` with updated run metadata (e.g., after completion). |
| `append_log` | Appends raw bytes to the run's `.log` file (creates the file on first call). |
| `read_log` | Reads the full log or the last `tail` lines. Returns an empty string if the file is missing. |
| `list_runs` | Lists all runs for a job with pagination; returns `(paginated_runs, total_count)`. |
| `cleanup` | Removes the oldest runs beyond `max_files`, deleting both `.log` and `.meta.json` for each. |

