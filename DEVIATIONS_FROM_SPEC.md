# Deviations from Spec

This document catalogs differences between `SPEC.md` (the original design specification) and the actual implementation as documented in `ARCHITECTURE.md`. Each entry includes the spec expectation, actual behavior, and severity.

---

## Features Not Implemented

### ~~1. PTY Emulation~~ ✅ FIXED

- **Spec**: "All process spawning uses PTY emulation for compatibility with programs that require a terminal environment." A `--no-pty` flag is described as an opt-in fallback.
- **Status**: **RESOLVED (intentional deviation)**. PTY emulation was intentionally removed. The production spawner uses piped I/O (`NoPtySpawner`) via `std::process::Command`, which reliably handles EOF on all platforms. `RealPtySpawner` dead code has been removed. Programs that check `isatty()` will see non-TTY mode, but this does not affect typical cron job workloads.

### ~~2. Auto Service Registration~~ ✅ FIXED

- **Spec**: "On first run, the daemon automatically registers itself as a system service" (Windows Service, launchd, systemd). `acs start` step 2: "If not registered as a system service, auto-register."
- **Status**: **IMPLEMENTED**. Running `acs start` without `--foreground` will now:
  1. Check if the service is already registered
  2. Auto-register if not (Task Scheduler on Windows, launchd plist on macOS, systemd user unit on Linux)
  3. Start the daemon as a background process
- **Note**: On Windows, Task Scheduler is used instead of Windows Service because Windows Services run as LOCAL SYSTEM with a minimal environment (missing USERPROFILE, APPDATA, user PATH entries, etc.), causing child processes like `claude` to fail. Task Scheduler runs as the current user, inheriting their full environment.

### ~~3. Background Daemonization~~ ✅ FIXED

- **Spec**: `acs start` without `--foreground` should start the daemon as a background service.
- **Status**: **IMPLEMENTED**. The daemon now starts in the background when `--foreground` is not specified:
  - **Windows**: Spawns the daemon as a hidden background process (`CREATE_NO_WINDOW`). Task Scheduler handles auto-start at user logon.
  - **macOS**: Uses `launchctl start com.acs.scheduler`
  - **Linux**: Uses `systemctl --user start acs.service`
- **Note**: The `--foreground` flag runs the daemon directly in the current process (useful for development and debugging).

### 4. Graceful Child Process Shutdown

- **Spec**: 9-step shutdown sequence including: send SIGTERM to all running child processes, wait up to 30 seconds for completion, SIGKILL any remaining processes, then mark runs as Killed.
- **Actual**: Shutdown sends a watch channel signal, marks all in-flight `JobRun` records as `Killed`, and releases the PID file. Child processes are not explicitly signaled (no SIGTERM sent, no 30-second grace period, no SIGKILL escalation). Child processes are orphaned when the daemon exits.
- **Impact**: Running jobs may be left as orphan processes on the OS after daemon shutdown. On Unix, orphaned children are reparented to init. On Windows, they continue running independently.
- **Severity**: Medium. Affects cleanup correctness when jobs are actively running during shutdown.

### 5. Log File Truncation

- **Spec**: "Output exceeds max size: Truncate log file, append `[LOG TRUNCATED]` marker, process continues." `max_log_file_size` (default 10 MB) is specified in `DaemonConfig`.
- **Actual**: `max_log_file_size` exists in the config struct but the log writer in `src/daemon/executor.rs` does not check file size during writes. Log files grow without bound.
- **Impact**: A job producing large output (e.g., verbose builds, data dumps) will consume unbounded disk space per run. Log rotation (`max_log_files_per_job`) limits the number of runs but not per-run size.
- **Severity**: Medium. Disk exhaustion risk for long-running or verbose jobs.

### 6. Delete Job Kills Active Run

- **Spec**: "DELETE /api/jobs/{id} -> 204, kills active run if any."
- **Actual**: `delete_job` in `src/server/routes.rs` removes the job from the store and broadcasts `JobChanged(Removed)` but does not check `active_runs` or send a kill signal to any running execution.
- **Impact**: Deleting a job while it is running leaves the run executing. The run will complete and attempt to update a job that no longer exists. Orphaned log files are cleaned up on next daemon startup.
- **Severity**: Low. Edge case that only matters when deleting actively running jobs.

### ~~7. `acs stop --force`~~ ✅ FIXED

- **Spec**: `acs stop [--force]` should force-kill the daemon process.
- **Status**: **IMPLEMENTED**. `acs stop --force` now:
  1. Reads the PID file from the data directory
  2. Sends SIGKILL (Unix) or uses `taskkill /F` (Windows) to terminate the process
  3. Cleans up the PID file

### ~~8. `acs uninstall --purge`~~ ✅ FIXED

- **Spec**: `acs uninstall --purge` should remove the system service registration and delete all data (data directory, logs, config).
- **Status**: **IMPLEMENTED**. `acs uninstall --purge` now:
  1. Stops the daemon (via API or service manager)
  2. Removes the system service registration
  3. Deletes the entire data directory when `--purge` is specified

---

## Behavior Differs from Spec

### 9. Corrupted jobs.json Recovery

- **Spec**: "Refuse to start, prompt user to fix or delete."
- **Actual**: If `jobs.json` fails to parse, it is silently renamed to `jobs.json.bak` and the store starts with an empty job list. A warning is logged but the daemon continues.
- **Rationale**: The implementation chose availability over strictness -- the daemon starts rather than blocking on manual intervention.
- **Severity**: Low. Data is preserved in the `.bak` file but the user may not notice the recovery unless they check daemon logs.

### 10. Invalid Cron/Timezone Auto-Disable

- **Spec**: "Invalid cron in jobs.json: Skip job, log error, auto-disable." The job should be persisted back with `enabled: false`.
- **Actual**: Jobs with invalid cron expressions or timezones are logged and skipped on each scheduler tick but never written back as disabled. They remain `enabled: true` and are re-evaluated (and re-skipped) every cycle.
- **Impact**: Minor log noise on each scheduler tick for permanently-broken jobs. No functional impact since the jobs never execute.
- **Severity**: Low.

### 11. Script Path Resolution

- **Spec**: "Relative to `{data_dir}/scripts/`. No `..` traversal. Absolute paths accepted but flagged as non-portable."
- **Actual**: `ScriptFile` paths are passed directly to the shell command builder with no path resolution relative to `{data_dir}/scripts/` and no traversal protection.
- **Impact**: Scripts must be specified with full paths or paths relative to the daemon's working directory, not relative to a `scripts/` subdirectory. No path traversal validation is performed.
- **Severity**: Low. The daemon binds to localhost only, so the attack surface is limited to local users who already have system access.

---

## Config/Environment Not Implemented

### 12. `ACS_LOG_LEVEL` Environment Variable

- **Spec**: `ACS_LOG_LEVEL` controls the tracing filter level (default: `info`).
- **Actual**: Not implemented. Log level is controlled only by the `-v` / `--verbose` CLI flag, which toggles between `info` and `debug`. There is no way to set `warn`, `error`, or `trace` levels.
- **Severity**: Low. The `-v` flag covers the most common use case (debug logging).

### 13. Coverage Enforcement in CI

- **Spec**: "90% line coverage, 85% branch coverage" enforced via `cargo-llvm-cov` in CI with Codecov integration.
- **Actual**: CI runs `cargo test`, `cargo clippy -- -D warnings`, `cargo fmt -- --check`, and a release build. `cargo-llvm-cov` is not installed or invoked. No coverage thresholds are enforced.
- **Severity**: Low. The test suite has 269 tests with broad coverage, but the threshold is not formally measured or enforced.

---

## Dead Dependencies

### 14. `fs4` Crate

- **Spec**: "Atomic file writes use `fs4` for locking + write-to-temp + rename pattern."
- **Actual**: `fs4` (version 0.13, tokio feature) is listed in `Cargo.toml` but is not imported or used anywhere in the source code. The JSON job store uses atomic tmp+rename without file locking.
- **Impact**: Unnecessary compile-time dependency. No functional impact.
- **Severity**: Trivial.

---

## Status Code Deviations

### 15. 503 Shutting Down

- **Spec**: Lists 503 as a status code for requests received during shutdown.
- **Actual**: No middleware or handler returns 503. During shutdown, the Axum server stops accepting new connections via graceful shutdown rather than responding with 503.
- **Severity**: Trivial. The behavior is functionally equivalent -- clients get a connection refused rather than a 503 response.

---

## Additions Not in Spec

### 16. `dispatch_tx` in AppState

- **Not in spec**: The spec's `AppState` definition does not include a dispatch channel.
- **Actual**: `AppState` contains `dispatch_tx: Option<mpsc::Sender<Job>>` which allows the trigger endpoint to dispatch jobs to the executor via the same channel the scheduler uses.
- **Rationale**: Necessary for the trigger API to function. The spec's trigger endpoint (`POST /api/jobs/{id}/trigger -> 202`) implies this capability but doesn't specify the mechanism.

### 17. Per-Job `timeout_secs`

- **Not in spec**: The spec defines `default_timeout_secs` only on `DaemonConfig`. Individual jobs have no timeout field.
- **Actual**: The `Job` struct includes `pub timeout_secs: u64`. If non-zero, it overrides the config-level default. If zero, the config default is used.
- **Rationale**: Useful addition that allows fine-grained timeout control per job.

### 18. Per-Job `log_environment`

- **Not in spec**: The spec does not include an option to log environment variables before job execution.
- **Actual**: Jobs have a `log_environment: bool` field (default `false`). When enabled, all inherited environment variables (the daemon's environment merged with job-specific `env_vars` overrides) are dumped into the run log before the command executes. Configurable via CLI (`--log-env`), REST API (`log_environment: true`), and web UI checkbox.
- **Rationale**: Necessary for diagnosing environment differences between service and user contexts (e.g., Windows Task Scheduler vs foreground mode).

---

## Summary

| Category | Count | Items |
|---|---|---|
| Not implemented | 5 | child shutdown, log truncation, delete-kills-run, ~~stop --force~~, ~~uninstall --purge~~ |
| ~~Fixed~~ | 5 | ~~PTY~~, ~~auto-service~~, ~~daemonize~~, stop --force, uninstall --purge |
| Different behavior | 3 | Corrupted recovery, invalid cron handling, script paths |
| Config not implemented | 2 | ACS_LOG_LEVEL, coverage enforcement |
| Dead dependencies | 1 | fs4 |
| Missing status codes | 1 | 503 |
| Additions beyond spec | 3 | dispatch_tx, per-job timeout, per-job log_environment |
| **Total remaining deviations** | **12** | |

### Recently Fixed
- **PTY Emulation** (#1): Intentionally resolved -- production spawner uses piped I/O (`NoPtySpawner`), `RealPtySpawner` dead code removed
- **Auto Service Registration** (#2): `acs start` now auto-registers and starts via platform service manager (Task Scheduler on Windows, launchd on macOS, systemd on Linux)
- **Background Daemonization** (#3): Daemon starts as a hidden background process (Windows) or via service manager (macOS/Linux) when `--foreground` is not specified
- **`acs stop --force`** (#7): Now reads PID file and force-kills the daemon process
- **`acs uninstall --purge`** (#8): Now removes the data directory when `--purge` is specified
