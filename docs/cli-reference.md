# ACS CLI Reference

Complete reference for the Agent Cron Scheduler command-line interface.

## Synopsis

```
agentcronsystem [OPTIONS] <COMMAND>
```

ACS is a cross-platform cron scheduler daemon. Most commands communicate with the daemon over HTTP. The exception is `agentcronsystem start`, which either runs the daemon directly (foreground mode) or spawns it as a background process. If no subcommand is provided, the help text is printed.

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
agentcronsystem --host 192.168.1.100 --port 9999 status

# Global options can also appear after the subcommand
agentcronsystem status --host 10.0.0.1 --port 1234

# Enable verbose output
agentcronsystem -v status
```

---

## Daemon Commands

### `agentcronsystem start`

Start the ACS daemon. By default the daemon is spawned as a background process and a system service is registered for auto-start at logon. If the daemon is already running, the command prints a message and exits successfully.

```
agentcronsystem start [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--foreground` | `-f` | flag | `false` | Run in foreground (do not daemonize) |
| `--config` | `-c` | `String` | none | Path to configuration file |
| `--port` | `-p` | `u16` | none | Port to listen on (overrides config and global `--port`) |
| `--data-dir` | | `String` | none | Data directory path |

#### Behavior

- **Background mode** (default): Registers a system service for auto-start and spawns the daemon, then polls `/health` for up to 3 seconds to confirm the daemon is responsive. The background process is started with `agentcronsystem start --foreground` only — `--config`, `--port`, and `--data-dir` are **not forwarded**. Use config files or environment variables to pass configuration to the background daemon (see [Configuration](configuration.md)).
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
agentcronsystem start

# Start in foreground mode with a custom config
agentcronsystem start -f -c /etc/acs/config.json

# Start on a custom port with a specific data directory
agentcronsystem start -p 9000 --data-dir /var/acs

# Combine short flags
agentcronsystem start -f -c /etc/acs.json -p 8080
```

---

### `agentcronsystem stop`

Stop the running ACS daemon. By default, sends a graceful shutdown request via the HTTP API (`POST /api/shutdown`). If the API is unreachable and a system service is registered, the service is stopped instead.

```
agentcronsystem stop [OPTIONS]
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
agentcronsystem stop

# Force kill the daemon
agentcronsystem stop --force
```

---

### `agentcronsystem status`

Show the current status of the ACS daemon by querying the `/health` endpoint.

```
agentcronsystem status
```

#### Options

This command has no subcommand-specific options. Use the global `--verbose` (`-v`) flag to print the raw JSON response.

#### Output Fields

- **Daemon Status** -- health status string (e.g., "ok")
- **Data Dir** -- path to the data directory
- **Web UI** -- URL for the web dashboard
- **Jobs** -- `active_jobs` (count of *enabled* workflows) and `total_jobs` (total registered workflows). Field names retain the legacy 'jobs' naming for compatibility; values are workflow counts.
- **Uptime** -- human-readable uptime (e.g., "1d 2h 30m 15s")
- **Version** -- daemon version string
- **Update** -- indicates if an update is available with version number, or "up to date". Shows "(could not check)" if GitHub API is unreachable (only visible with `--verbose`)
- **Service** -- system service registration status, sourced from the `service` block on the `/health` response

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Daemon is running and healthy |
| 1 | Daemon returned an error, or is unreachable |

#### Examples

```sh
# Basic status check
agentcronsystem status

# Verbose status with raw JSON
agentcronsystem -v status

# Check status of a remote daemon
agentcronsystem --host 192.168.1.50 status
```

---

### `agentcronsystem restart`

Restart the daemon by sending a `POST /api/restart` request. After the restart is initiated, the CLI polls `/health` for up to 10 seconds waiting for the new process to respond.

```
agentcronsystem restart
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
agentcronsystem restart
```

---

### `agentcronsystem update`

Update the ACS daemon to the latest release version from GitHub. Downloads the appropriate platform-specific binary, creates a backup of the current executable, and replaces it in-place. The new version takes effect on restart.

```
agentcronsystem update [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--version` | | `String` | latest | Target version (e.g., `4.2.5` or `v4.2.5`). If omitted, checks GitHub for the latest release. |
| `--force` | | flag | `false` | Force update even if already on the target version |

#### Platform Support

Auto-update is supported on:
- **Windows x86_64** -- downloads `agentcronsystem-windows-x86_64.exe`
- **macOS aarch64** -- downloads `agentcronsystem-macos-aarch64`
- **macOS x86_64** -- downloads `agentcronsystem-macos-x86_64`
- **Linux x86_64** -- downloads `agentcronsystem-linux-x86_64`

Other platforms are not supported and will return an error.

#### Behavior

1. **Version check** -- If `--version` is omitted, fetches the latest release from GitHub's API
2. **Already up-to-date** -- If the target version matches the current version and `--force` is not set, prints a message and exits successfully
3. **Download** -- Downloads the release binary from GitHub and displays download progress
4. **Backup** -- Renames the current executable to `.bak` (removes any existing backup first)
5. **Replace** -- Moves the downloaded binary into place. If this fails, the backup is restored
6. **Permissions** -- On Unix systems (macOS, Linux), sets execute permissions (`0o755`) on the new binary

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Update completed successfully (including no update needed if already on target version) |
| 1 | Error: unsupported platform, network failure, download failed, or binary replacement failed |

#### Examples

```sh
# Update to the latest version (checks GitHub)
agentcronsystem update

# Update to a specific version
agentcronsystem update --version 4.2.4

# Force update even if already on the latest version
agentcronsystem update --force
```

---

### `agentcronsystem uninstall`

Uninstall the ACS service. This stops the daemon (gracefully via API, falling back to ending the system task), removes the system service registration, and optionally purges all data.

```
agentcronsystem uninstall [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--purge` | | flag | `false` | Also remove all data (workflows, logs, the entire data directory) |

#### Behavior

Stops the daemon, removes the system service registration, and optionally purges data. See [Service Registration](service-registration.md) for platform-specific details.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Uninstall completed (warnings may be printed for non-critical failures) |

#### Examples

```sh
# Uninstall service registration only
agentcronsystem uninstall

# Uninstall and delete all data
agentcronsystem uninstall --purge
```

---

## Workflows Commands

### `agentcronsystem workflows`

Manage scheduled workflows. All subcommands communicate with the running daemon over HTTP.

```
agentcronsystem workflows <SUBCOMMAND>
```

---

### `agentcronsystem workflows list`

List all workflows. By default shows all workflows in a table format.

```
agentcronsystem workflows list [OPTIONS]
```

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--enabled` | | flag | `false` | Show only enabled workflows (conflicts with `--disabled`) |
| `--disabled` | | flag | `false` | Show only disabled workflows (conflicts with `--enabled`) |
| `--json` | | flag | `false` | Output as JSON |

#### Output Columns (Table Mode)

| Column | Description |
|--------|-------------|
| NAME | Workflow name. If name length > 15 chars, truncated to 12 chars and `...` appended (final display width 15). |
| ENABLED | `true` or `false` |
| SCHEDULE | Cron expression. If length > 19 chars, truncated to 16 chars and `...` appended (final display width 19). |
| LAST RUN | Relative time of last execution |
| NEXT RUN | Relative time of next scheduled execution |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | List retrieved successfully |
| 1 | Error (e.g., daemon unreachable) |

#### Examples

```sh
# List all workflows
agentcronsystem workflows list

# List only enabled workflows
agentcronsystem workflows list --enabled

# List only disabled workflows
agentcronsystem workflows list --disabled

# Output as JSON for scripting
agentcronsystem workflows list --json
```

---

### `agentcronsystem workflows get`

Show one workflow by UUID or name.

```
agentcronsystem workflows get [OPTIONS] <ID_OR_NAME>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<ID_OR_NAME>` | `String` | Workflow UUID or name |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--json` | | flag | `false` | Output as JSON |

#### Output Fields (Default Mode)

Prints key/value lines including: ID, Name, Version, Schedule, Timezone (if set), Enabled, Steps (count), Last run (relative), Last status, Next run (relative), Created, Updated.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Workflow retrieved successfully |
| 1 | Error (e.g., workflow not found, daemon unreachable) |

#### Examples

```sh
# Show a workflow by name
agentcronsystem workflows get my-pipeline

# Show as JSON
agentcronsystem workflows get my-pipeline --json

# Show by UUID
agentcronsystem workflows get 550e8400-e29b-41d4-a716-446655440000
```

---

### `agentcronsystem workflows create`

Create a new workflow from a JSON file or inline JSON string.

```
agentcronsystem workflows create [OPTIONS]
```

Exactly one of `--file` or `--json` must be provided.

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--file` | | `String` | none | Path to a JSON file containing the workflow definition (conflicts with `--json`) |
| `--json` | | `String` | none | Inline JSON string containing the workflow definition (conflicts with `--file`) |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Workflow created successfully |
| 1 | Error (e.g., duplicate name, invalid JSON, daemon unreachable) |

#### Examples

```sh
# Create from a file
agentcronsystem workflows create --file /path/to/workflow.json

# Create from inline JSON
agentcronsystem workflows create --json '{"name":"heartbeat","schedule":"* * * * *","steps":[{"kind":"shell","id":"ping","command":"echo alive"}]}'
```

---

### `agentcronsystem workflows update`

Update an existing workflow from a JSON file, inline JSON, or the convenience `--enable` / `--disable` flags. At least one of `--file`, `--json`, `--enable`, or `--disable` must be provided. Precedence at runtime: `--file` > `--json` > `--enable`/`--disable`. Clap enforces `--file`/`--json` mutual exclusion and `--enable`/`--disable` mutual exclusion, but does NOT prevent combining file/json with enable/disable — the runtime precedence resolves that case.

```
agentcronsystem workflows update [OPTIONS] <ID_OR_NAME>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<ID_OR_NAME>` | `String` | Workflow UUID or name |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--file` | | `String` | none | Path to a JSON file containing the update fields (conflicts with `--json`) |
| `--json` | | `String` | none | Inline JSON string containing the update fields (conflicts with `--file`) |
| `--enable` | | flag | `false` | Convenience: set `enabled=true` (conflicts with `--disable`) |
| `--disable` | | flag | `false` | Convenience: set `enabled=false` (conflicts with `--enable`) |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Workflow updated successfully |
| 1 | Error (e.g., workflow not found, invalid JSON, daemon unreachable) |

#### Examples

```sh
# Enable a workflow
agentcronsystem workflows update my-pipeline --enable

# Disable a workflow
agentcronsystem workflows update my-pipeline --disable

# Update from a file (partial patch — only supplied fields are changed)
agentcronsystem workflows update my-pipeline --file /path/to/update.json

# Update via inline JSON
agentcronsystem workflows update my-pipeline --json '{"schedule":"0 3 * * *"}'
```

---

### `agentcronsystem workflows delete`

Delete a workflow. Prompts for confirmation unless `-y` is provided.

```
agentcronsystem workflows delete [OPTIONS] <ID_OR_NAME>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<ID_OR_NAME>` | `String` | Workflow UUID or name |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--yes` | `-y` | flag | `false` | Skip confirmation prompt |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Workflow deleted, or deletion cancelled by user |
| 1 | Error (e.g., workflow not found, daemon unreachable) |

#### Examples

```sh
# Delete with interactive confirmation
agentcronsystem workflows delete my-pipeline

# Delete without confirmation
agentcronsystem workflows delete my-pipeline -y

# Delete by UUID without confirmation
agentcronsystem workflows delete 550e8400-e29b-41d4-a716-446655440000 -y
```

---

### `agentcronsystem workflows trigger`

Manually trigger an immediate run of a workflow, regardless of its cron schedule. Optionally provide per-invocation parameters.

```
agentcronsystem workflows trigger [OPTIONS] <ID_OR_NAME>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<ID_OR_NAME>` | `String` | Workflow UUID or name |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--input` | | `String` | none | Input JSON (any valid JSON value) — replaces the workflow's `default_input` for this run (conflicts with `--input-file`) |
| `--input-file` | | `String` | none | Path to a file containing input JSON (conflicts with `--input`) |
| `--env` | `-e` | `String` | none | Per-trigger environment variable in `KEY=VALUE` format (repeatable); merges with the workflow's `env_vars`, trigger values win on collision |
| `--target-step` | | `String` | none | Step `id` to route the trigger's `--input` to as stdin. Strings are written as raw bytes; all other JSON is serialized as compact JSON. Only `Shell` and `Script` steps consume stdin; other step kinds ignore the value silently. Overrides `pass_stdin` for the matching step. |
| `--follow` | | flag | `false` | After triggering, stream SSE events until `RunCompleted` or `RunFailed` |

#### Behavior

- **Without `--follow`**: Triggers the workflow and returns immediately with a confirmation message that includes the run ID.
- **With `--follow`**: Opens an SSE connection (`/api/events/workflows?workflow_id=<id>`) before triggering (to avoid race conditions with fast-completing runs), then streams events to stdout until `RunCompleted` or `RunFailed`. Events are filtered by `run_id`; output from concurrent runs of the same workflow does not interleave.

**Template substitution.** Step fields that support templates use `${input.<path>}` to reference the trigger input and `${steps.<step_id>.<accessor>}` to reference prior step outputs. The `--input` value becomes the `input` namespace in those templates.

**Concurrency rejection.** When the workflow has `allow_concurrent: false` and a run is already active, the daemon returns `409 Conflict` (`error: "concurrent_run_active"`) and the command fails with a `concurrent run active` error. The active run is left untouched. Wait for it to finish before retrying. To kill an active run, POST to `/api/runs/{run_id}/kill` (no CLI equivalent currently exists).

#### Output

Without `--follow`, the command prints:

```
Workflow '<workflow_id>' triggered (run: <run_id>).
```

The placeholder is the UUID returned by the server (not the name passed on the CLI). With `--follow`, an additional line `Following events (run_id=<run_id>)...` is printed.

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Workflow triggered (and stream ended, if `--follow` was used). Note: exit code 0 indicates the CLI operation succeeded, not that the workflow itself succeeded. |
| 1 | Error (e.g., workflow not found, invalid input JSON, or the workflow has `allow_concurrent: false` with a run already active — the daemon returns `409 Conflict` with `error: "concurrent_run_active"`). |

#### Examples

```sh
# Trigger a workflow
agentcronsystem workflows trigger my-pipeline

# Trigger and stream events in real time
agentcronsystem workflows trigger my-pipeline --follow

# Trigger with input JSON
agentcronsystem workflows trigger my-pipeline --input '{"repo":"my-repo","branch":"main"}'

# Trigger with input from a file
agentcronsystem workflows trigger my-pipeline --input-file /path/to/payload.json

# Trigger with per-run environment variables
agentcronsystem workflows trigger my-pipeline -e "ENV=staging" -e "DRY_RUN=true"

# Route the trigger's input bytes to a specific step's stdin
agentcronsystem workflows trigger my-pipeline --input '"raw text payload"' --target-step ingest

# Combine options
agentcronsystem workflows trigger my-pipeline --input '{"prompt":"summarize"}' -e "MODEL=claude" --follow
```

---

### `agentcronsystem workflows runs`

List recent runs for a workflow.

```
agentcronsystem workflows runs [OPTIONS] <NAME_OR_ID>
```

#### Arguments

| Argument | Type | Description |
|----------|------|-------------|
| `<NAME_OR_ID>` | `String` | Workflow UUID or name |

#### Options

| Option | Short | Type | Default | Description |
|--------|-------|------|---------|-------------|
| `--limit` | | `usize` | `20` | Max number of runs to return (capped at 100 by the server) |
| `--offset` | | `usize` | `0` | Skip the first N runs (for pagination) |
| `--json` | | flag | `false` | Output as JSON |

#### Output Columns (Table Mode)

| Column | Description |
|--------|-------------|
| RUN_ID | UUID of the run |
| STATUS | Run status (`Running`, `Completed`, `Failed`, or `Killed`) |
| STARTED | Relative time of when the run started |
| DURATION_MS | Total duration in milliseconds, or `-` if still running |
| COST_USD | Total cost in USD across all agent steps, or `-` if none |

#### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Runs retrieved successfully |
| 1 | Error (e.g., workflow not found, daemon unreachable) |

#### Examples

```sh
# List last 20 runs (default)
agentcronsystem workflows runs my-pipeline

# List last 5 runs
agentcronsystem workflows runs my-pipeline --limit 5

# Paginate: skip the first 20 and return the next 10
agentcronsystem workflows runs my-pipeline --limit 10 --offset 20

# Output as JSON
agentcronsystem workflows runs my-pipeline --json
```

---

## Connection Errors

When the daemon is not reachable, all commands that communicate with it display the following error message:

```
Could not connect to daemon at <host>:<port>. Is it running? (try: agentcronsystem start)
```

Use `agentcronsystem start` to start the daemon, or check that the `--host` and `--port` values match the running daemon's configuration.

## Default Daemon Address

The default daemon address is `http://127.0.0.1:8377`. Override this with the global `--host` and `--port` options, or use the `--port` (`-p`) flag on `agentcronsystem start` to launch the daemon on a different port.

## Common Recipes

```sh
# Start the daemon and verify it is healthy
agentcronsystem start
agentcronsystem status

# Create a workflow and trigger it immediately, watching live output
agentcronsystem workflows create --file my-pipeline.json
agentcronsystem workflows trigger my-pipeline --follow

# Disable a workflow temporarily, then re-enable it
agentcronsystem workflows update my-pipeline --disable
agentcronsystem workflows update my-pipeline --enable

# Inspect the last 10 runs of a workflow
agentcronsystem workflows runs my-pipeline --limit 10

# Delete a workflow without prompting
agentcronsystem workflows delete my-pipeline -y

# Connect to a remote daemon
agentcronsystem --host 10.0.0.5 --port 8377 workflows list
```
