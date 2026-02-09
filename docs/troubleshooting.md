# ACS Troubleshooting Guide

This guide covers common issues you may encounter when running the Agent Cron Scheduler (ACS) and how to resolve them.

Many troubleshooting steps reference files in the ACS data directory. See [Configuration](configuration.md#data-directory-locations) for platform-specific paths and override options, and [Storage](storage.md#1-data-directory-layout) for the full file layout.

---

## 1. Daemon Won't Start

### Stale PID File

**Symptom:** You see an error like `Daemon is already running (PID 12345). PID file: <path>` but no daemon process is actually running.

**Cause:** The daemon previously crashed or was killed without performing a graceful shutdown, leaving behind a stale `acs.pid` file.

**How ACS Detects Stale PIDs:**
- On **Unix**: ACS calls `kill(pid, 0)` (signal 0), which checks whether the process exists without actually sending a signal.
- On **Windows**: ACS calls `OpenProcess` with `PROCESS_QUERY_LIMITED_INFORMATION` to check if the process handle is valid.

If the recorded PID is still alive, ACS waits up to 10 seconds (20 retries at 500ms intervals) for it to exit before giving up. This handles graceful restart scenarios where the old process is shutting down.

**Solution:**
1. Verify the old process is truly not running:
   - Windows: `tasklist | findstr acs`
   - Unix: `ps aux | grep acs`
2. Manually delete the PID file:
   - Windows: `del "%LOCALAPPDATA%\agent-cron-scheduler\acs.pid"`
   - macOS: `rm ~/Library/Application\ Support/agent-cron-scheduler/acs.pid`
   - Linux: `rm ~/.local/share/agent-cron-scheduler/acs.pid`
3. Restart the daemon: `acs start`

Alternatively, use force stop which handles PID file cleanup automatically:
```
acs stop --force
acs start
```

### Port Already in Use

**Symptom:** Error message `Failed to bind to 127.0.0.1:8377` when starting the daemon.

**Cause:** Another process is already using the configured port, or a previous ACS instance did not shut down cleanly.

**Solution:**
1. Check what is using the port:
   - Windows: `netstat -ano | findstr :8377`
   - Unix: `lsof -i :8377` or `ss -tlnp | grep 8377`
2. If another ACS instance is running, stop it: `acs stop`
3. If a different process is using the port, change the ACS port:
   - Via CLI flag: `acs start --port 9000`
   - Via config file: Set `"port": 9000` in `config.json`
   - Via the `acs.port` file: Check `{data_dir}/acs.port` to see the current port

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

### Windows (Task Scheduler)

**Symptom:** `Warning: Could not register auto-start` when running `acs start`.

**Possible causes:**
- Insufficient permissions. Try running your terminal as Administrator.
- The `schtasks` command is not available or is blocked by group policy.

**Quick check:** `schtasks /Query /TN AgentCronScheduler`

### macOS (launchd)

**Symptom:** The daemon does not start automatically after login, or `launchctl load` fails.

**Possible causes:**
- The plist file has incorrect XML syntax. Validate with: `plutil ~/Library/LaunchAgents/com.acs.scheduler.plist`
- The executable path in the plist no longer exists (e.g., after moving the binary).
- SIP (System Integrity Protection) or privacy settings may block background processes.

**Quick check:** `launchctl list | grep com.acs.scheduler`

### Linux (systemd user unit)

**Symptom:** The daemon does not start at boot or stops when you log out.

**Possible causes:**
- **Linger not enabled.** By default, systemd kills user services when the user logs out. Fix: `loginctl enable-linger $USER`
- The unit file references an incorrect executable path.
- systemd user instance is not running. Check with: `systemctl --user status`

**Quick check:** `systemctl --user status acs.service`
**View service logs:** `journalctl --user -u acs.service`

---

## 3. Job Execution Problems

### Job Not Running

**Symptom:** A job exists but never executes.

**Checklist:**
1. **Is the job enabled?** Check with `acs list` or the web UI. Disabled jobs are skipped by the scheduler.
2. **Is the cron expression correct?** Verify the schedule field. ACS uses standard 5-field cron syntax (`minute hour day-of-month month day-of-week`). An invalid expression results in a `Cron error`.
3. **Is the timezone correct?** If a job has a `timezone` field set, the scheduler uses that timezone for next-run calculations. An incorrect timezone string may cause unexpected scheduling.
4. **Is the daemon running?** Confirm with `acs status`. Jobs only execute while the daemon is active.
5. **Was the job recently created or updated?** The scheduler recalculates next-run times when notified of changes. Check the `next_run_at` field in `acs list --json` or the API response (`GET /api/jobs`).

### Job Times Out

**Symptom:** A job run shows status `Failed` with error `execution timed out`.

**Cause:** The job ran longer than its configured timeout.

Timeout resolution follows a per-job then daemon-default fallback. See [Job Management](job-management.md#timeouts) for details.

**Solution:**
- Increase the timeout for the specific job via the API:
  ```
  curl -X PATCH http://127.0.0.1:8377/api/jobs/<job-name> -H "Content-Type: application/json" -d '{"timeout_secs": 3600}'
  ```
- Or set a higher global default in `config.json`:
  ```json
  { "default_timeout_secs": 3600 }
  ```
- Set to `0` for no timeout:
  ```
  curl -X PATCH http://127.0.0.1:8377/api/jobs/<job-name> -H "Content-Type: application/json" -d '{"timeout_secs": 0}'
  ```

### Process Spawn Failures

**Symptom:** Job run immediately fails with `Failed to spawn process: ...`

**Possible causes:**
- **Missing shell:** ACS executes shell commands via `cmd.exe /C` on Windows or `/bin/sh -c` on Unix. If these are not available in the execution environment, spawning fails.
- **Script file not found:** For `ScriptFile` execution types, the script path must exist and be executable.
- **Windows PowerShell scripts:** `.ps1` files are executed via `powershell.exe -File`. Ensure PowerShell is available and execution policy permits running scripts.
- **Permission issues:** The user running ACS must have permission to execute the command.

**Solution:**
1. Test the command manually in a terminal to verify it works.
2. For script files, verify the path is absolute or relative to the working directory.
3. Check the job's run log for the specific error message: `acs logs <job-name>`

### Wrong Working Directory

**Symptom:** Job fails because it cannot find files or produces output in the wrong location.

**Cause:** The `working_dir` field on the job is not set or points to a nonexistent directory.

**Solution:**
- Set or update the working directory via the API:
  ```
  curl -X PATCH http://127.0.0.1:8377/api/jobs/<job-name> -H "Content-Type: application/json" -d '{"working_dir": "/path/to/directory"}'
  ```
- Verify the path exists and is accessible by the user running the daemon.

### Environment Variable Issues

**Symptom:** A job behaves differently when run by ACS compared to running manually in a terminal.

**Cause:** The daemon process may not have the same environment as your interactive shell. Key variables like `PATH`, `HOME`, or custom variables may differ.

**Solution:**
1. Enable environment logging to see what the job receives:
   ```
   curl -X PATCH http://127.0.0.1:8377/api/jobs/<job-name> -H "Content-Type: application/json" -d '{"log_environment": true}'
   ```
   Then trigger a run and check the logs. The output will include a full dump of all environment variables under the `=== Environment ===` header.
2. Set explicit environment variables on the job:
   ```
   curl -X PATCH http://127.0.0.1:8377/api/jobs/<job-name> -H "Content-Type: application/json" -d '{"env_vars": {"MY_VAR": "value"}}'
   ```

---

## 4. Log-Related Issues

### Log Files Missing

**Symptom:** `acs logs <job-name>` returns empty or you cannot find log files on disk.

**Cause:** Logs are stored at `{data_dir}/logs/{job_id}/`. If the job has never run, no logs exist.

**Solution:**
1. Verify the job has run at least once: `acs list` shows `last_run_at`.
2. Check the logs directory directly:
   - Windows: `%LOCALAPPDATA%\agent-cron-scheduler\logs\`
   - macOS: `~/Library/Application Support/agent-cron-scheduler/logs/`
   - Linux: `~/.local/share/agent-cron-scheduler/logs/`
3. Older logs may have been cleaned up. ACS retains a maximum of `max_log_files_per_job` (default: 50) run logs per job. Older runs are deleted after each new run completes.

### Large daemon.log

**Symptom:** The `daemon.log` file is consuming significant disk space.

**How ACS manages it:**
- The daemon log is automatically size-managed. When `daemon.log` exceeds 1 GB, ACS drops the oldest 25% of the file content, keeping the newest 75%.
- On daemon startup, `daemon.log` is truncated (each daemon session starts with a fresh log).

**Solution:**
- Restart the daemon (`acs restart`) to truncate `daemon.log` -- each daemon session starts with a fresh log.
- Do **not** delete `daemon.log` while the daemon is running. The daemon holds the file descriptor open; on Unix, deleting the file creates an invisible unlinked inode that continues consuming disk space. On Windows, the delete will likely fail because the file is locked.
- If the automatic size-managed truncation fails for any reason (e.g., file permissions), ACS logs a warning to stderr but continues operating.

### Orphaned Log Directories

**Symptom:** The `logs/` directory contains subdirectories for jobs that no longer exist.

**Cause:** Log directories from deleted jobs may remain if the daemon was not running when the job was deleted, or if cleanup was interrupted.

**Solution:**
- ACS cleans up orphaned log directories automatically on every daemon startup. It compares UUID-named subdirectories in `logs/` against known job IDs and removes any that no longer match.
- To trigger this cleanup, restart the daemon: `acs restart`
- Non-UUID directories inside `logs/` are left untouched.

---

## 5. Data Corruption

### Corrupted jobs.json

**Symptom:** Jobs are missing after a crash, or the daemon logs `jobs.json is corrupted`.

**What happens automatically:**
When ACS detects that `jobs.json` contains invalid JSON, it:
1. Creates a backup at `jobs.json.bak` (preserving the corrupted data).
2. Logs a warning about the corruption.
3. Starts with an empty job list.

**Recovery from backup:**
1. Stop the daemon: `acs stop`
2. Navigate to the data directory.
3. Examine the backup: open `jobs.json.bak` in a text editor.
4. If the backup looks recoverable (e.g., minor corruption), fix the JSON and save it as `jobs.json`.
5. If the backup is beyond repair, you will need to recreate your jobs.
6. Restart the daemon: `acs start`

**Prevention:**
- ACS uses atomic writes (write to `.tmp` file, then rename) to prevent partial-write corruption during normal operation. Corruption is typically caused by hardware issues, disk-full conditions, or forceful termination at the exact moment of a write.

### Missing Data Directory

**Symptom:** The daemon starts but reports it cannot find or create the data directory.

**What happens automatically:**
ACS creates the data directory and its subdirectories (`logs/`, `scripts/`) on startup if they do not exist. This includes creating all intermediate parent directories.

**Solution:**
- If creation fails, check filesystem permissions on the parent directory.
- On Windows, ensure `%LOCALAPPDATA%` is set (it is required and ACS will panic if missing).
- You can specify a custom data directory: `acs start --data-dir /path/to/custom/dir`

---

## 6. Debugging Tools

### Verbose Daemon Output

Run the daemon in the foreground with debug logging to see detailed output:

```
acs start --foreground -v
```

Alternatively, when running WITHOUT `-v`, you can use `RUST_LOG` for fine-grained control over log filtering (e.g., `RUST_LOG=acs=debug,tower_http=warn acs start --foreground`). Note: the `-v` flag initializes its own tracing subscriber, so `RUST_LOG` is ignored when `-v` is present. Use one or the other, not both.

This produces verbose log lines to both stderr and `daemon.log`, including:
- Config resolution steps
- PID file acquisition
- Scheduler tick calculations
- Job dispatch and execution events
- HTTP request handling

### Check Daemon Health

```
acs status
```

This contacts the daemon's `/health` endpoint and displays:
- Daemon status (`"ok"` when healthy)
- Data directory path
- Web UI URL
- Active and total job counts
- Uptime
- Version
- Service registration status

For raw JSON output, use the global `-v` flag:
```
acs -v status
```
Note: `-v` also enables debug-level tracing, so the raw JSON may be interspersed with debug log lines from HTTP and other subsystems.

### View Job Logs

```
acs logs <job-name>
```

This retrieves the output from the job's most recent run. For a richer log viewing experience with full run history, use the REST API (`GET /api/jobs/{id}/runs`) or the [API Reference](api-reference.md).

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

This returns a JSON object with daemon status, version, uptime, job counts, and data directory information.

---

## 7. Common CLI Errors

### "Could not connect to daemon at 127.0.0.1:8377. Is it running? (try: acs start)"

**Cause:** The daemon is not running, or it is running on a different host/port.

**Solution:**
1. Start the daemon: `acs start`
2. If you configured a non-default port, pass it to CLI commands: `acs --port 9000 status`
3. Check the `acs.port` file in the data directory to see the actual port.

### "Daemon is already running (PID ...)"

**Cause:** Another daemon instance is active, or a stale PID file exists.

**Solution:** See the [Stale PID File](#stale-pid-file) section above.

### "Not found: Job with id '...' not found"

**Cause:** The specified job ID or name does not match any existing job.

**Solution:**
1. List available jobs: `acs list`
2. Use the exact job name or UUID as shown in the listing.
3. Job names are case-sensitive.

### "Conflict: A job with name '...' already exists"

**Cause:** You are trying to create or rename a job with a name that is already taken.

**Solution:** Choose a different name, or delete/rename the existing job first.

### "Validation error: ..."

**Cause:** A job field has an invalid value. Common cases:
- Invalid cron expression syntax.
- Invalid UUID format.
- Empty or missing required fields.

**Solution:** Check the error message for specifics and correct the field value.

### "Daemon failed to start"

**Cause:** The background daemon process was spawned but did not respond to health checks within 3 seconds.

**Solution:**
1. Check the daemon log for startup errors:
   - Windows: `%LOCALAPPDATA%\agent-cron-scheduler\daemon.log`
   - macOS: `~/Library/Application Support/agent-cron-scheduler/daemon.log`
   - Linux: `~/.local/share/agent-cron-scheduler/daemon.log`
2. Try running in the foreground for immediate error output: `acs start --foreground`
3. Common root causes: port conflict, permission issues, corrupted config file.

### "Daemon failed to come back up after restart"

**Cause:** The `acs restart` command stopped the old daemon but the new daemon process did not respond to health checks within 10 seconds (20 retries at 500ms intervals).

**Solution:**
1. Check the daemon log for startup errors (see [Daemon Log Location](#daemon-log-location) above).
2. Try a manual stop-and-start cycle:
   ```
   acs stop
   acs start
   ```
3. If that also fails, try running in the foreground to see the error: `acs start --foreground`
4. Common root causes: port conflict (old process still releasing the port), permission issues, corrupted config file.

### "Request failed: ..."

**Cause:** A network error occurred while communicating with the daemon that is not a simple connection failure.

**Solution:** Check that no firewall or proxy is interfering with local HTTP requests to `127.0.0.1`.

