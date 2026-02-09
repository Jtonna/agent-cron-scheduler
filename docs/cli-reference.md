# ACS CLI Reference

Complete reference for the Agent Cron Scheduler command-line interface.

## Synopsis

```
acs [OPTIONS] <COMMAND>
```

ACS is a cross-platform cron scheduler daemon. Most commands communicate with the daemon over HTTP. The exception is `acs start`, which either runs the daemon directly (foreground mode) or spawns it as a background process. If no subcommand is provided, the help text is printed.

## Global Options

These options are available on every subcommand. They can appear before or after the subcommand name.

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--host` | | `String` | `127.0.0.1` | Daemon host address |
| `--port` | | `u16` | `8377` | Daemon port |
| `--verbose` | `-v` | flag | `false` | Enable verbose output |
| `--version` | `-V` | flag | | Print version and exit |
| `--help` | `-h` | flag | | Print help and exit |

### Examples

```sh
# Connect to a daemon on a different host
acs --host 192.168.1.100 --port 9999 status

# Global options can also appear after the subcommand
acs status --host 10.0.0.1 --port 1234

# Enable verbose output
acs -v status
```

---

## Daemon Commands

### `acs start`

Start the ACS daemon. By default the daemon is spawned as a background process and a system service is registered for auto-start at logon. If the daemon is already running, the command prints a message and exits successfully.

```
acs start [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--foreground` | `-f` | flag | `false` | Run in foreground (do not daemonize) |
| `--config` | `-c` | `String` | none | Path to configuration file |
| `--port` | `-p` | `u16` | none | Port to listen on (overrides config and global `--port`) |
| `--data-dir` | | `String` | none | Data directory path |

#### Behavior

- **Background mode** (default): Registers a system service for auto-start and spawns the daemon, then polls `/health` for up to 3 seconds to confirm the daemon is responsive. The background process is started with `acs start --foreground` only — `--config`, `--port`, and `--data-dir` are **not forwarded**. Use config files or environment variables to pass configuration to the background daemon (see [Configuration](configuration.md)).
- **Foreground mode** (`-f`): Runs the daemon directly in the current process, blocking until terminated. CLI flags `--config`, `--port`, and `--data-dir` are applied in this mode.

See [Service Registration](service-registration.md) for platform-specific details on how the daemon is started and managed.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Daemon started successfully, or was already running |
| 1 | Daemon was spawned but failed to respond on the expected port |

#### Examples

```sh
# Start the daemon in the background
acs start

# Start in foreground mode with a custom config
acs start -f -c /etc/acs/config.json

# Start on a custom port with a specific data directory
acs start -p 9000 --data-dir /var/acs

# Combine short flags
acs start -f -c /etc/acs.json -p 8080
```

---

### `acs stop`

Stop the running ACS daemon. By default, sends a graceful shutdown request via the HTTP API (`POST /api/shutdown`). If the API is unreachable and a system service is registered, the service is stopped instead.

```
acs stop [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--force` | | flag | `false` | Force kill the daemon process via PID file |

#### Behavior

- **Graceful mode** (default): Sends `POST /api/shutdown` to the daemon. Falls back to stopping the system service if the API is unreachable.
- **Force mode** (`--force`): Terminates the daemon process directly via the PID file.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Daemon stopped successfully |
| 1 | Error: API returned an error, or daemon is unreachable and no system service is registered |

#### Examples

```sh
# Graceful shutdown
acs stop

# Force kill the daemon
acs stop --force
```

---

### `acs status`

Show the current status of the ACS daemon by querying the `/health` endpoint.

```
acs status
```

#### Options

This command has no subcommand-specific options. Use the global `--verbose` (`-v`) flag to print the raw JSON response.

#### Output Fields

- **Daemon Status** -- health status string (e.g., "ok")
- **Data Dir** -- path to the data directory
- **Web UI** -- URL for the web dashboard
- **Jobs** -- count of active and total jobs
- **Uptime** -- human-readable uptime (e.g., "1d 2h 30m 15s")
- **Version** -- daemon version string
- **Service** -- system service registration status

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Daemon is running and healthy |
| 1 | Daemon returned an error, or is unreachable |

#### Examples

```sh
# Basic status check
acs status

# Verbose status with raw JSON
acs -v status

# Check status of a remote daemon
acs --host 192.168.1.50 status
```

---

### `acs restart`

Restart the daemon by sending a `POST /api/restart` request. After the restart is initiated, the CLI polls `/health` for up to 10 seconds waiting for the new process to respond.

```
acs restart
```

#### Options

This command has no subcommand-specific options.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Daemon restarted successfully |
| 1 | Error: the restart API call failed, or the daemon did not respond within 10 seconds after restart |

#### Examples

```sh
acs restart
```

---

### `acs uninstall`

Uninstall the ACS service. This stops the daemon (gracefully via API, falling back to ending the system task), removes the system service registration, and optionally purges all data.

```
acs uninstall [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--purge` | | flag | `false` | Also remove all data (jobs, logs, the entire data directory) |

#### Behavior

Stops the daemon, removes the system service registration, and optionally purges data. See [Service Registration](service-registration.md) for platform-specific details.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Uninstall completed (warnings may be printed for non-critical failures) |

#### Examples

```sh
# Uninstall service registration only
acs uninstall

# Uninstall and delete all data
acs uninstall --purge
```

---

## Job Commands

### `acs add`

> **Note:** This subcommand is parsed on all platforms, but its handler is gated behind `#[cfg(target_os = "windows")]`. On macOS and Linux, the command is accepted by the CLI parser but execution returns an error. Use the REST API (`POST /api/jobs`) or the web UI to create jobs on non-Windows platforms.

Create a new scheduled job. Exactly one of `--cmd` or `--script` must be specified.

```
acs add [OPTIONS] --name <NAME> --schedule <SCHEDULE>
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--name` | `-n` | `String` | **required** | Job name (must be unique) |
| `--schedule` | `-s` | `String` | **required** | Cron schedule expression (5-field) |
| `--cmd` | `-c` | `String` | none | Shell command to execute (conflicts with `--script`) |
| `--script` | | `String` | none | Script file path to execute (conflicts with `--cmd`). Paths are passed verbatim to the shell interpreter with no resolution relative to `data_dir/scripts/`. |
| `--timezone` | | `String` | UTC | IANA timezone name (e.g., `America/New_York`) |
| `--working-dir` | | `String` | none | Working directory for the command |
| `--env` | `-e` | `String` | none | Environment variable in `KEY=VALUE` format (repeatable) |
| `--disabled` | | flag | `false` | Create the job in a disabled state |
| `--log-env` | | flag | `false` | Include full environment variables in run logs |

The schedule uses standard 5-field cron syntax. See [Job Management](job-management.md#cron-expressions) for format details and examples.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Job created successfully |
| 1 | Error (e.g., duplicate name, invalid schedule, daemon unreachable) |

#### Examples

```sh
# Add a job that runs every minute
acs add -n heartbeat -s "* * * * *" -c "echo alive"

# Add a daily backup job at 2:30 AM Eastern
acs add -n backup -s "30 2 * * *" -c "/usr/local/bin/backup.sh" --timezone America/New_York

# Add a job with environment variables
acs add -n deploy -s "0 4 * * 1" -c "deploy.sh" -e "ENV=production" -e "VERBOSE=true"

# Add a script-based job
acs add -n cleanup -s "0 * * * *" --script cleanup.sh

# Add a job in disabled state with a working directory
acs add -n build -s "*/15 * * * *" -c "make build" --working-dir /home/user/project --disabled

# Add a job with environment logging enabled
acs add -n audit -s "0 0 * * *" -c "run-audit.sh" --log-env
```

---

### `acs remove`

Remove a scheduled job by name or UUID. Prompts for confirmation unless `--yes` is provided.

```
acs remove [OPTIONS] <JOB>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<JOB>` | `String` | Job name or UUID |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--yes` | | flag | `false` | Skip confirmation prompt |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Job removed, or removal cancelled by user |
| 1 | Error (e.g., job not found, daemon unreachable) |

#### Examples

```sh
# Remove a job (interactive confirmation)
acs remove heartbeat

# Remove without confirmation
acs remove heartbeat --yes

# Remove by UUID
acs remove 550e8400-e29b-41d4-a716-446655440000 --yes
```

---

### `acs list`

List all scheduled jobs. By default shows all jobs in a table format.

```
acs list [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--enabled` | | flag | `false` | Show only enabled jobs (conflicts with `--disabled`) |
| `--disabled` | | flag | `false` | Show only disabled jobs (conflicts with `--enabled`) |
| `--json` | | flag | `false` | Output as JSON |

#### Output Columns (Table Mode)

| Column | Description |
|--------|-------------|
| NAME | Job name (truncated to 13 characters) |
| SCHEDULE | Cron expression |
| ENABLED | `true` or `false` |
| LAST RUN | Relative time of last execution |
| NEXT RUN | Relative time of next scheduled execution |
| LAST EXIT | Exit code of the most recent run |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | List retrieved successfully |
| 1 | Error (e.g., daemon unreachable) |

#### Examples

```sh
# List all jobs
acs list

# List only enabled jobs
acs list --enabled

# List only disabled jobs
acs list --disabled

# Output as JSON for scripting
acs list --json
```

---

### `acs enable`

Enable a disabled scheduled job so it resumes running on its cron schedule.

```
acs enable <JOB>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<JOB>` | `String` | Job name or UUID |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Job enabled successfully |
| 1 | Error (e.g., job not found) |

#### Examples

```sh
acs enable backup
acs enable 550e8400-e29b-41d4-a716-446655440000
```

---

### `acs disable`

Disable a scheduled job so it stops running on its cron schedule. The job is not deleted and can be re-enabled later.

```
acs disable <JOB>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<JOB>` | `String` | Job name or UUID |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Job disabled successfully |
| 1 | Error (e.g., job not found) |

#### Examples

```sh
acs disable backup
acs disable 550e8400-e29b-41d4-a716-446655440000
```

---

### `acs trigger`

Manually trigger an immediate run of a job, regardless of its cron schedule.

```
acs trigger [OPTIONS] <JOB>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<JOB>` | `String` | Job name or UUID |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--follow` | | flag | `false` | Follow the job output in real time via SSE (Server-Sent Events) |

#### Behavior

- Without `--follow`: Triggers the job and returns immediately with a confirmation message.
- With `--follow`: Opens an SSE connection (filtered by `job_id`) before triggering the job (to avoid race conditions with fast-completing jobs), then streams output to stdout until the job completes or fails. Note: the stream filters by `job_id`, not `run_id`, so if multiple runs of the same job are active, their output may interleave.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Job triggered (and stream ended, if `--follow` was used). Note: exit code 0 indicates the CLI operation succeeded, not that the job itself succeeded. |
| 1 | Error (e.g., job not found) |

#### Examples

```sh
# Trigger a job
acs trigger backup

# Trigger and watch output in real time
acs trigger backup --follow
```

---

## Log Commands

### `acs logs`

View run history and log output for a specific job.

```
acs logs [OPTIONS] <JOB>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<JOB>` | `String` | Job name or UUID |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--follow` | | flag | `false` | Follow live output via SSE (Ctrl+C to stop) |
| `--run` | | `String` | none | Specific run ID to view log output for |
| `--last` | | `usize` | `20` | Show last N runs in the run list. |
| `--tail` | | `usize` | none | Show last N lines of log output (only with `--run`) |
| `--json` | | flag | `false` | Output as JSON |

#### Modes of Operation

1. **List runs** (default): When neither `--follow` nor `--run` is specified, displays a table of recent runs for the job, limited by `--last` (default 20).
2. **View run log** (`--run <RUN_ID>`): Displays the full log output for a specific run. Use `--tail` to limit to the last N lines.
3. **Follow live** (`--follow`): Opens an SSE stream and prints job output events in real time. Shows start markers, output text, completion status, and error messages. This is a long-lived stream that does not auto-terminate on job completion; use Ctrl+C to stop.

#### Output Columns (Run List Mode)

| Column | Description |
|--------|-------------|
| RUN ID | UUID of the run |
| STARTED | Timestamp of when the run started |
| STATUS | Run status (e.g., "completed", "failed", "running") |
| EXIT | Exit code, or `-` if not applicable |
| SIZE | Log file size in human-readable format |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Logs retrieved successfully |
| 1 | Error (e.g., job not found, run not found) |

#### Examples

```sh
# List recent runs for a job (default: last 20)
acs logs backup

# List the last 5 runs
acs logs backup --last 5

# View log output for a specific run
acs logs backup --run 550e8400-e29b-41d4-a716-446655440000

# View only the last 100 lines of a run's log
acs logs backup --run 550e8400-e29b-41d4-a716-446655440000 --tail 100

# Follow live output for a job
acs logs backup --follow

# Output run list as JSON
acs logs backup --json

# Output a specific run's log as JSON
acs logs backup --run 550e8400-e29b-41d4-a716-446655440000 --json
```

---

## Connection Errors

When the daemon is not reachable, all commands that communicate with it display the following error message:

```
Could not connect to daemon at <host>:<port>. Is it running? (try: acs start)
```

Use `acs start` to start the daemon, or check that the `--host` and `--port` values match the running daemon's configuration.

## Default Daemon Address

The default daemon address is `http://127.0.0.1:8377`. Override this with the global `--host` and `--port` options, or use the `--port` (`-p`) flag on `acs start` to launch the daemon on a different port.
