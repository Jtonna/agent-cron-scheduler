# ACS Troubleshooting Guide

This guide covers common issues you may encounter when running the Agent Cron Scheduler (ACS) and how to resolve them.

Many troubleshooting steps reference files in the ACS data directory. See [Configuration](configuration.md#data-directory-locations) for platform-specific paths and override options, and [Storage](storage.md#1-data-directory-layout) for the full file layout.

---

## 1. Daemon Won't Start

### Stale PID File

**Symptom:** You see an error like `Daemon is already running (PID 12345). PID file: <path>` but no daemon process is actually running.

**Cause:** The daemon previously crashed or was killed without performing a graceful shutdown, leaving behind a stale `agentcronsystem.pid` file.

**How ACS Detects Stale PIDs:**
- On **Unix**: ACS calls `kill(pid, 0)` (signal 0), which checks whether the process exists without actually sending a signal.
- On **Windows**: ACS calls `OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION`, then verifies the process is truly alive via `GetExitCodeProcess`. This two-step check is necessary because `OpenProcess` can succeed on a dead process if another process (e.g., the Electron parent app or Windows Task Scheduler) still holds a handle to it, keeping the kernel object alive. If `GetExitCodeProcess` returns an exit code other than `STILL_ACTIVE` (259), the process is considered dead.

If the recorded PID is still alive, ACS waits up to 10 seconds (20 retries at 500ms intervals) for it to exit before giving up. This handles graceful restart scenarios where the old process is shutting down.

**Solution:**
1. Verify the old process is truly not running:
   - Windows: `tasklist | findstr agentcronsystem`
   - Unix: `ps aux | grep agentcronsystem`
2. Manually delete the PID file:
   - Windows: `del "%LOCALAPPDATA%\agent-cron-scheduler\agentcronsystem.pid"`
   - macOS: `rm ~/Library/Application\ Support/agent-cron-scheduler/agentcronsystem.pid`
   - Linux: `rm ~/.local/share/agent-cron-scheduler/agentcronsystem.pid`
3. Restart the daemon: `agentcronsystem start`

Alternatively, use force stop which handles PID file cleanup automatically:
```
agentcronsystem stop --force
agentcronsystem start
```

### Port Already in Use

**Symptom:** Error message `Failed to bind to 127.0.0.1:8377` when starting the daemon.

**Cause:** Another process is already using the configured port, or a previous ACS instance did not shut down cleanly.

**Solution:**
1. Check what is using the port:
   - Windows: `netstat -ano | findstr :8377`
   - Unix: `lsof -i :8377` or `ss -tlnp | grep 8377`
2. If another ACS instance is running, stop it: `agentcronsystem stop`
3. If a different process is using the port, change the ACS port:
   - Via CLI flag: `agentcronsystem start --port 9000`
   - Via config file: Set `"port": 9000` in `config.json`
   - Via the `agentcronsystem.port` file: Check `{data_dir}/agentcronsystem.port` to see the current port

### Config File Errors

**Symptom:** Error like `Failed to parse config file` or `Failed to parse config from ...` on startup.

**Cause:** The `config.json` file contains invalid JSON or has incorrect field types.

ACS searches for configuration in a 5-level priority order. See [Configuration](configuration.md#config-file-resolution-order) for the full resolution chain.

**Solution:**
1. Validate your JSON syntax. Common mistakes include trailing commas and unquoted keys.
2. If you passed an explicit `--config` path that does not exist, ACS will fail with `Config file not found`.
3. To start with defaults, temporarily rename or delete the broken config file. See [Configuration](configuration.md#complete-example) for the full default values.

---

## 2. Service Registration Issues

ACS registers itself as a user-level service for auto-start at login. See [Service Registration](service-registration.md) for full platform-specific details (service names, file locations, install/uninstall commands).

### Windows (Registry Run Key)

**Symptom:** `Warning: Could not register auto-start` when running `agentcronsystem start`.

**Possible causes:**
- Registry write failed. Check that `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` is writable.
- Antivirus or group policy blocking registry writes to the Run key.

**Quick check:** `reg query "HKCU\Software\Microsoft\Windows\CurrentVersion\Run" /v AgentCronScheduler`

### macOS (launchd)

**Symptom:** The daemon does not start automatically after login, or `launchctl load` fails.

**Possible causes:**
- The plist file has incorrect XML syntax. Validate with: `plutil ~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist`
- The executable path in the plist no longer exists (e.g., after moving the binary).
- SIP (System Integrity Protection) or privacy settings may block background processes.

**Quick check:** `launchctl list | grep com.agentcronsystem.scheduler`

### Linux (systemd user unit)

**Symptom:** The daemon does not start at boot or stops when you log out.

**Possible causes:**
- **Linger not enabled.** By default, systemd kills user services when the user logs out. Fix: `loginctl enable-linger $USER`
- The unit file references an incorrect executable path.
- systemd user instance is not running. Check with: `systemctl --user status`

**Quick check:** `systemctl --user status agentcronsystem.service`
**View service logs:** `journalctl --user -u agentcronsystem.service`

---

## 3. Workflow Execution Problems

### Workflow Not Running

**Symptom:** A workflow exists but never executes.

**Checklist:**
1. **Is the workflow enabled?** Check with `agentcronsystem workflows list` or the web UI. Disabled workflows are skipped by the scheduler.
2. **Is the cron expression correct?** Verify the schedule field. ACS uses standard 5-field cron syntax (`minute hour day-of-month month day-of-week`). An invalid expression results in a `Cron error` logged at startup.
3. **Is the timezone correct?** If a workflow has a `timezone` field set, the scheduler uses that timezone for next-run calculations. An incorrect timezone string will cause the cron to be skipped entirely for that tick.
4. **Is the daemon running?** Confirm with `agentcronsystem status`. Workflow runs only execute while the daemon is active.
5. **Was the workflow recently created or updated?** The scheduler recalculates next-run times when notified of changes. Check the `next_run_at` field in `agentcronsystem workflows list --json` or the API response (`GET /api/workflows`).

### Workflow Run Times Out

**Symptom:** A workflow run shows status `Failed` with error `execution timed out` on a step.

**Cause:** A step ran longer than its configured `timeout_secs`. Timeouts are per-step only; there is no workflow-level timeout.

**Solution:**
- Update the step's `timeout_secs` via `PATCH /api/workflows/<name>` with the updated step definition.
- Set `timeout_secs` to `0` or omit it for no timeout on a specific step.

### Process Spawn Failures

**Symptom:** A workflow run immediately fails with `Failed to spawn process: ...` on a Shell or Script step.

**Possible causes:**
- **Missing shell:** Shell steps execute via `cmd.exe /C` on Windows or `/bin/sh -c` on Unix. If the shell is not available in the execution environment, spawning fails.
- **Script file not found:** For Script steps, the script path must exist and be accessible by the daemon process at the time the step runs.
- **Windows PowerShell scripts:** `.ps1` files are executed via `pwsh -File` when `pwsh` (PowerShell 7+ Core) is on the system PATH. If `pwsh` is not found, ACS falls back to `powershell.exe` (Windows PowerShell 5.1) automatically. Ensure the execution policy permits running scripts (`Set-ExecutionPolicy RemoteSigned` or `Bypass` for the current user).
- **Permission issues:** The user running the ACS daemon must have execute permission on the command or script.

**Solution:**
1. Test the command manually in a terminal to verify it works.
2. For script files, verify the path is absolute or correct relative to the workflow's `working_dir`.
3. Check the workflow run log for the specific error: `GET /api/runs/<run_id>` or `agentcronsystem workflows runs <name>`.

### Wrong Working Directory

**Symptom:** A step fails because it cannot find files or produces output in the wrong location.

**Cause:** The `working_dir` field on the workflow (or step-level override) is not set or points to a nonexistent directory.

**Solution:**
- Set or update the working directory via `PATCH /api/workflows/<name>` with a `working_dir` value in the workflow body.
- Verify the path exists and is accessible by the user running the daemon.
- Steps can override the workflow-level `working_dir` via `StepDefCommon.working_dir`.

### Environment Variable Issues

**Symptom:** A step behaves differently when run by ACS compared to running manually in a terminal.

**Cause:** The daemon process may not have the same environment as your interactive shell. Key variables like `PATH`, `HOME`, or custom variables may differ.

**Solution:**
1. Set explicit environment variables on the workflow:
   ```
   PATCH /api/workflows/<name>
   {"env_vars": {"MY_VAR": "value"}}
   ```
2. Use the trigger `env` overlay to pass per-run environment overrides:
   ```
   POST /api/workflows/<name>/trigger
   {"input": {}, "env": {"MY_VAR": "override"}}
   ```
3. Step-level `env_vars` are merged with the workflow-level vars (step wins on collisions).

---

## 4. Workflow Runtime Notes

This section describes runtime behaviors that are relevant when building or debugging workflows.

### Killing Cron-Fired Runs

`POST /api/runs/{id}/kill` works for both manually triggered runs and cron-fired runs. The scheduler registers each dispatched run's kill sender in the shared `kill_signals` registry, so the kill endpoint can signal any run regardless of how it was started.

### `allow_concurrent: false` with `schedule_mode: Cron`

When `allow_concurrent: false` and `schedule_mode` is `Cron`, the scheduler kills the in-progress run before dispatching the new tick's run. The scheduler sends the kill signal to the active run, waits up to 5 seconds for it to terminate, then dispatches the new run. If the active run does not terminate within 5 seconds, the new run is dispatched anyway.

To skip dispatch instead of killing: set `schedule_mode` to `WaitForCompletion`. That mode skips any cron tick that fires while a run is active.

### `StepRun.step_index` in Run Records

`StepRun.step_index` reflects the 1-based runtime execution order. The first step to execute in a run has `step_index: 1`, the second has `step_index: 2`, and so on. Branch steps inside a `MatchStep` continue the counter from where the match step left off. This value matches the `step_index` field in `StepStarted` and `StepCompleted` SSE events.

### `pass_stdin` Source Selection

When `pass_stdin: true`, a Shell or Script step receives the stdout of the step that executed immediately before it in the run. The step context tracks outputs in insertion order, so the immediately-prior step's output is always the one piped to stdin.

---

## 5. Log-Related Issues

### Log Files Missing

**Symptom:** You cannot find log files for a workflow run on disk.

**Cause:** Logs are stored at `{data_dir}/logs/{workflow_id}/{run_id}.log`. If the run has never executed, no log file exists. If the workflow was recently migrated from an older ACS installation, the old per-job log layout (`{data_dir}/logs/{job_id}/`) may still be present on disk alongside the new layout.

**Solution:**
1. Verify the workflow has run at least once: `agentcronsystem workflows list` shows `last_run_at`.
2. Check the logs directory directly:
   - Windows: `%LOCALAPPDATA%\agent-cron-scheduler\logs\`
   - macOS: `~/Library/Application Support/agent-cron-scheduler/logs/`
   - Linux: `~/.local/share/agent-cron-scheduler/logs/`
3. Each run produces one combined log file per run ID under the workflow's UUID subdirectory.

### Large daemon.log

**Symptom:** The `daemon.log` file is consuming significant disk space.

**How ACS manages it:**
- The daemon log is automatically size-managed. When `daemon.log` exceeds 1 GB, ACS drops the oldest 25% of the file content, keeping the newest 75%.
- On daemon startup, `daemon.log` is truncated (each daemon session starts with a fresh log).

**Solution:**
- Restart the daemon (`agentcronsystem restart`) to truncate `daemon.log` — each daemon session starts with a fresh log.
- Do **not** delete `daemon.log` while the daemon is running. The daemon holds the file descriptor open; on Unix, deleting the file creates an invisible unlinked inode that continues consuming disk space. On Windows, the delete will likely fail because the file is locked.
- If the automatic size-managed truncation fails for any reason (e.g., file permissions), ACS logs a warning to stderr but continues operating.

### Orphaned Log Directories

**Symptom:** The `logs/` directory contains subdirectories for workflows that no longer exist.

**Cause:** Log directories from deleted workflows may remain if the daemon was not running when the workflow was deleted, or if cleanup was interrupted.

**Solution:**
- Orphaned log directories must be cleaned up manually. There is no automatic cleanup code in `start_daemon`; restarting the daemon does not trigger any such cleanup.
- Non-UUID directories inside `logs/` are left untouched.
- To remove orphaned directories, identify workflow UUIDs that no longer exist in `workflows.json` and delete their corresponding subdirectories under `logs/` and `runs/`.

---

## 6. Data Corruption

### Corrupted workflows.json

**Symptom:** Workflows are missing after a crash, or the daemon logs a warning about `workflows.json` being corrupted.

**What happens automatically:**
When ACS detects that `workflows.json` contains invalid JSON, it:
1. Creates a timestamped backup (e.g., `workflows.json.bak.<timestamp>`).
2. Logs a warning about the corruption.
3. Starts with an empty workflow list.

**Recovery from backup:**
1. Stop the daemon: `agentcronsystem stop`
2. Navigate to the data directory.
3. Examine the backup in a text editor.
4. If recoverable (e.g., minor corruption), fix the JSON and save it as `workflows.json`.
5. If beyond repair, you will need to recreate your workflows.
6. Restart the daemon: `agentcronsystem start`

**Prevention:**
- ACS uses atomic writes (write to `.tmp` file, then rename) to prevent partial-write corruption during normal operation. Corruption is typically caused by hardware issues, disk-full conditions, or forceful termination at the exact moment of a write.

### Corrupted runs/index.json

**Symptom:** `GET /api/runs/{id}` returns 404 for a run that you know was created, or the daemon logs `runs/index.json is corrupted`.

**What happens automatically:**
`FsWorkflowRunStore` maintains a `runs/index.json` file that maps `run_id → workflow_id` for fast lookups. On startup, if `index.json` is invalid JSON, ACS:
1. Creates a timestamped backup (`index.json.bak.<timestamp>`) in the `runs/` directory.
2. Rebuilds the index by scanning all `runs/<workflow_id>/<run_id>.json` files on disk.
3. Persists the rebuilt index.

This recovery is automatic and requires no manual intervention. All previously-completed run records are recovered as long as their individual JSON files are intact.

**If individual run files are corrupted:**
- Corrupted individual run files are skipped with a warning during `list_runs`; they do not prevent other runs from loading.
- You can manually delete a corrupted run file and its entry will be absent from future listings.

### Missing Data Directory

**Symptom:** The daemon starts but reports it cannot find or create the data directory.

**What happens automatically:**
ACS creates the data directory and its subdirectories (`logs/`, `scripts/`, `runs/`) on startup if they do not exist. This includes creating all intermediate parent directories.

**Solution:**
- If creation fails, check filesystem permissions on the parent directory.
- On Windows, ensure `%LOCALAPPDATA%` is set (it is required and ACS will panic if missing).
- You can specify a custom data directory: `agentcronsystem start --data-dir /path/to/custom/dir`

---

## 7. Debugging Tools

### Verbose Daemon Output

Run the daemon in the foreground with debug logging to see detailed output:

```
agentcronsystem start --foreground -v
```

Alternatively, when running WITHOUT `-v`, you can use `RUST_LOG` for fine-grained control over log filtering (e.g., `RUST_LOG=agentcronsystem=debug,tower_http=warn agentcronsystem start --foreground`). Note: the `-v` flag initializes its own tracing subscriber, so `RUST_LOG` is ignored when `-v` is present. Use one or the other, not both.

When `-v` is used, `main.rs` initializes the global tracing subscriber (stderr-only, debug level) before `start_daemon` runs. This causes `start_daemon`'s own subscriber initialization (which includes the `daemon.log` file layer) to silently fail via `try_init()`, because a global subscriber is already set. As a result, `-v` produces verbose log lines to **stderr only** (not to `daemon.log`). Without `-v`, `start_daemon` successfully initializes its subscriber with both stderr and file layers, and `RUST_LOG` controls the log level for both outputs.

Verbose output includes:
- Config resolution steps
- PID file acquisition
- Scheduler tick calculations
- Workflow dispatch and execution events
- HTTP request handling

### Check Daemon Health

```
agentcronsystem status
```

This contacts the daemon's `/health` endpoint and displays:
- Daemon status (`"ok"` when healthy)
- Data directory path
- Web UI URL
- Active and total workflow counts
- Uptime
- Version
- Service registration status

For raw JSON output, use the global `-v` flag:
```
agentcronsystem -v status
```
Note: `-v` also enables debug-level tracing, so the raw JSON may be interspersed with debug log lines from HTTP and other subsystems.

### View Workflow Run Logs

Retrieve a run record and associated log output via the REST API:
```
GET /api/runs/{run_id}
GET /api/workflows/{id}/runs
```

Or use the CLI:
```
agentcronsystem workflows runs <name-or-id>
agentcronsystem workflows runs <name-or-id> --limit 5 --json
```

Run logs are stored at `{data_dir}/logs/{workflow_id}/{run_id}.log`.

### Daemon Log Location

The daemon's own log file is at:
- Windows: `%LOCALAPPDATA%\agent-cron-scheduler\daemon.log`
- macOS: `~/Library/Application Support/agent-cron-scheduler/daemon.log`
- Linux: `~/.local/share/agent-cron-scheduler/daemon.log`

### Health Endpoint

You can directly query the health endpoint for scripting or monitoring:

```
curl http://127.0.0.1:8377/health
```

This returns a JSON object with daemon status, version, uptime, workflow counts, and data directory information.

---

## 8. Common CLI Errors

### "Could not connect to daemon at 127.0.0.1:8377. Is it running? (try: agentcronsystem start)"

**Cause:** The daemon is not running, or it is running on a different host/port.

**Solution:**
1. Start the daemon: `agentcronsystem start`
2. If you configured a non-default port, pass it to CLI commands: `agentcronsystem --port 9000 status`
3. Check the `agentcronsystem.port` file in the data directory to see the actual port.

### "Daemon is already running (PID ...)"

**Cause:** Another daemon instance is active, or a stale PID file exists.

**Solution:** See the [Stale PID File](#stale-pid-file) section above.

### "Not found: Workflow with id '...' not found"

**Cause:** The specified workflow ID or name does not match any existing workflow.

**Solution:**
1. List available workflows: `agentcronsystem workflows list`
2. Use the exact workflow name or UUID as shown in the listing.
3. Workflow names are case-sensitive.

### "Conflict: A workflow with name '...' already exists"

**Cause:** You are trying to create or rename a workflow with a name that is already taken.

**Solution:** Choose a different name, or delete/rename the existing workflow first.

### "Validation error: ..."

**Cause:** A workflow field has an invalid value. Common cases:
- Invalid cron expression syntax.
- Invalid UUID format.
- Empty or missing required fields.

**Solution:** Check the error message for specifics and correct the field value.

### "Daemon failed to start"

**Cause:** The background daemon process was spawned but did not respond to health checks after 6 retries at 500ms intervals.

**Solution:**
1. Check the daemon log for startup errors:
   - Windows: `%LOCALAPPDATA%\agent-cron-scheduler\daemon.log`
   - macOS: `~/Library/Application Support/agent-cron-scheduler/daemon.log`
   - Linux: `~/.local/share/agent-cron-scheduler/daemon.log`
2. Try running in the foreground for immediate error output: `agentcronsystem start --foreground`
3. Common root causes: port conflict, permission issues, corrupted config file.

### "Daemon failed to come back up after restart"

**Cause:** The `agentcronsystem restart` command stopped the old daemon but the new daemon process did not respond to health checks within 10 seconds (20 retries at 500ms intervals).

**Solution:**
1. Check the daemon log for startup errors (see [Daemon Log Location](#daemon-log-location) above).
2. Try a manual stop-and-start cycle:
   ```
   agentcronsystem stop
   agentcronsystem start
   ```
3. If that also fails, try running in the foreground to see the error: `agentcronsystem start --foreground`
4. Common root causes: port conflict (old process still releasing the port), permission issues, corrupted config file.

### "Request failed: ..."

**Cause:** A network error occurred while communicating with the daemon that is not a simple connection failure.

**Solution:** Check that no firewall or proxy is interfering with local HTTP requests to `127.0.0.1`.
