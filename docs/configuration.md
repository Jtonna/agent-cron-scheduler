# ACS Configuration Guide

This document describes how to configure the Agent Cron Scheduler (ACS) daemon, including the config file format, field reference, CLI overrides, config file resolution, and examples.

## Overview

ACS reads a single JSON configuration file at startup. All fields are optional; any omitted field falls back to its built-in default. An empty JSON object (`{}`) is a valid configuration that uses all defaults.

The config file is **not** reloaded at runtime. Restart the daemon to apply changes.

## Config File Resolution

When the daemon starts, it searches for a configuration file in the following order. The first file found is used; if no file is found anywhere, built-in defaults are applied.

| Priority | Source | Path |
|---|---|---|
| 1 | `--config` CLI flag | Exact path provided via `agentcronsystem start --config <path>`. If specified but the file does not exist, the daemon exits with an error. |
| 2 | `ACS_CONFIG_DIR` environment variable | `$ACS_CONFIG_DIR/config.json` |
| 3 | Platform config directory | `<platform_config_dir>/agent-cron-scheduler/config.json` (see platform paths below) |
| 4 | Data directory | `<data_dir>/config.json`. The data directory used in this fallback is itself resolved using `ACS_DATA_DIR` if set. So if `ACS_DATA_DIR=/opt/acs/data`, Priority 4 looks at `/opt/acs/data/config.json`, not the platform default. |
| 5 | Built-in defaults | No file needed. Uses `DaemonConfig::default()` values as listed in the field reference. |

### Platform Config Directories (Priority 3)

The platform config directory is resolved using the `dirs::config_dir()` function:

| Platform | Config Directory |
|---|---|
| Windows | `%APPDATA%\agent-cron-scheduler\config.json` |
| macOS | `~/Library/Application Support/agent-cron-scheduler/config.json` |
| Linux | `~/.config/agent-cron-scheduler/config.json` |

### Important Behavior

- Priority 1 (`--config`): If you explicitly pass a config file path and it does not exist, the daemon returns an error and does not start. This is the only priority level that fails on a missing file.
- Priorities 2—4: If the resolved path does not exist, the daemon silently moves to the next priority level.
- Priority 5: Always succeeds. The daemon runs with all default values.

## Field Reference

| Field | Type | Default | Description |
|---|---|---|---|
| `host` | string | `"127.0.0.1"` | IP address the daemon HTTP server binds to. Use `"0.0.0.0"` to listen on all interfaces. |
| `port` | integer (u16) | `8377` | TCP port the daemon HTTP server listens on. |
| `data_dir` | string or null | `null` | Override the data directory path. When `null`, the platform default is used (see [Data Directory Locations](#data-directory-locations)). |
| `max_log_files_per_job` | integer | `50` | Maximum number of log files retained per workflow run history entry. Older files are cleaned up automatically. (Field name retains 'job' naming for compatibility; counts apply to workflow run logs. Currently not enforced by the workflow log layer — reserved for future use.) |
| `max_log_file_size` | integer (bytes) | `10485760` (10 MB) | Maximum size in bytes for individual run log files. **(Not currently enforced; reserved for future use.)** |
| `default_timeout_secs` | integer | `0` | Default step timeout in seconds. A value of `0` means no timeout. **(Reserved; not currently applied to workflow steps at runtime.)** |
| `broadcast_capacity` | integer | `4096` | Capacity of the internal broadcast channel used for workflow events (SSE streaming). |
| `pty_rows` | integer (u16) | `24` | Number of rows for the pseudo-terminal allocated to step processes. **(No effect; the production spawner uses piped I/O, not a PTY.)** |
| `pty_cols` | integer (u16) | `80` | Number of columns for the pseudo-terminal allocated to step processes. **(No effect; the production spawner uses piped I/O, not a PTY.)** |
| `default_allow_concurrent` | bool | `false` | Reserved default concurrency setting. New workflows default to `allow_concurrent: true` regardless of this config field. This config field is reserved for a future feature that would override that default at daemon scope; it is currently not consumed. |
| `default_schedule_mode` | string (`Cron` \| `WaitForCompletion`) | `"Cron"` | Reserved default schedule mode. **(Reserved; not currently applied to new workflows at runtime.)** |
| `display_timezone` | string | `"America/Los_Angeles"` | IANA timezone used for cost analytics calendar-day boundaries and as the default `timezone` for new workflows. |
| `display_workflow_dir_root` | string or null | `null` | Root directory for auto-created per-workflow working dirs. When `null`, falls back to `dirs::document_dir()/agent-cron-scheduler/<sanitized-name>`. |

### Partial Configuration

You only need to specify the fields you want to override. Unspecified fields use their defaults. For example, to change only the port and host:

```json
{
  "host": "0.0.0.0",
  "port": 9000
}
```

All other fields will use their default values.

## CLI Overrides

Several `agentcronsystem start` subcommand flags override config file values at startup. These flags are evaluated after the config file is loaded; they take the highest precedence.

| CLI Flag | Overrides Config Field | Notes |
|---|---|---|
| `start --config <path>` / `-c <path>` | Config file selection | Selects which config file to load (Priority 1 in resolution order). |
| `start --port <n>` / `-p <n>` | `port` | Sets the TCP port the daemon binds to. Takes precedence over the `port` field in the config file and the built-in default. |
| `start --data-dir <path>` | `data_dir` | Sets the data directory. Takes precedence over the `data_dir` field in the config file and the platform default. |

### Two `--port` Flags

There are two `--port` flags with different purposes:

- **Global `--port`** (e.g., `acs --port 9000 status`): Tells the CLI client which port to connect to when communicating with an already-running daemon. This does **not** affect which port the daemon listens on.
- **`agentcronsystem start --port` (`-p`)** (e.g., `agentcronsystem start -p 9000`): Sets the port the daemon binds to when starting.

### Host Override

The daemon's bind host is resolved with the following precedence (highest to lowest):

1. Global `--host` flag, **only when the value differs from the default** (`127.0.0.1`)
2. `host` value in the loaded config file
3. Built-in default (`127.0.0.1`)

> **Edge case:** Because the global `--host` flag uses `127.0.0.1` as its clap default, there is no way to distinguish between the user explicitly passing `--host 127.0.0.1` and not passing the flag at all. If your config file sets `host: “0.0.0.0”` and you run `agentcronsystem --host 127.0.0.1 start`, the config value (`0.0.0.0`) wins silently — the CLI flag is ignored because its value matches the default. To force `127.0.0.1` when the config sets a different host, remove or change the `host` field in the config file instead of relying on the CLI flag. (See `acs/src/daemon/mod.rs` — the host override applied in `start_daemon`.)

For the full list of CLI options, see [CLI Reference](cli-reference.md).

## Data Directory Locations

The data directory stores workflows, run logs, scripts, the PID file, the port file, and the daemon log. It is resolved in the following order:

| Priority | Source | Description |
|---|---|---|
| 1 | `--data-dir` CLI flag | Explicit path passed to `agentcronsystem start --data-dir <path>`. |
| 2 | `data_dir` field in config | The `data_dir` field in the loaded config file. |
| 3 | `ACS_DATA_DIR` environment variable | Override via environment variable. |
| 4 | Platform default | OS-specific default directory (see below). |

### Platform Default Data Directories

| Platform | Default Path | Notes |
|---|---|---|
| Windows | `%LOCALAPPDATA%\agent-cron-scheduler` | Per-user directory, no admin elevation required. Uses the `LOCALAPPDATA` environment variable. |
| macOS | `~/Library/Application Support/agent-cron-scheduler` | Resolved via `dirs::data_dir()`. Per-user directory. |
| Linux | `~/.local/share/agent-cron-scheduler` | Resolved via `dirs::data_dir()`. Per-user directory. |

On startup, the daemon ensures the data directory and its subdirectories (`logs/`, `scripts/`) exist. For the full data directory file layout, see [Storage](storage.md).

## Environment Variables

| Variable | Description |
|---|---|
| `ACS_DATA_DIR` | Override the data directory location. Takes effect when no `--data-dir` CLI flag and no `data_dir` config field is set. |
| `ACS_CONFIG_DIR` | Directory to search for `config.json`. Checked at priority 2 in the config resolution order, after the `--config` CLI flag but before platform and data directory fallbacks. |
| `RUST_LOG` | Controls the tracing/logging filter level for the **daemon process only** (not CLI client commands). Follows the `tracing_subscriber::EnvFilter` syntax. Examples: `info`, `debug`, `agentcronsystem=debug,tower=warn`. Defaults to `info` if not set. **Important:** The `-v` flag initializes its own tracing subscriber before the daemon starts, so `RUST_LOG` is silently ignored when `-v` is present. Use one or the other, not both. |
| `LOCALAPPDATA` | (Windows only) Used to determine the default data directory. This variable is set automatically by Windows and should not normally need to be changed. |

## Examples

### Minimal config — change only the port

```json
{
  "port": 9000
}
```

### Fully-specified config

```json
{
  "host": "127.0.0.1",
  "port": 8377,
  "data_dir": null,
  "max_log_files_per_job": 50,
  "max_log_file_size": 10485760,
  "default_timeout_secs": 0,
  "broadcast_capacity": 4096,
  "pty_rows": 24,
  "pty_cols": 80,
  "default_allow_concurrent": false,
  "default_schedule_mode": "Cron",
  "display_timezone": "America/Los_Angeles",
  "display_workflow_dir_root": null
}
```

### Start with all defaults

```bash
agentcronsystem start
```

The daemon binds to `127.0.0.1:8377` and stores data in the platform default directory.

### Start with a custom config file

```bash
agentcronsystem start --config /etc/acs/config.json
```

### Start with a custom data directory and port

```bash
agentcronsystem start --data-dir /var/lib/acs --port 9000
```

### Start in foreground mode for debugging

```bash
RUST_LOG=debug agentcronsystem start --foreground
```

### Use environment variables for configuration

```bash
export ACS_DATA_DIR=/opt/acs/data
export ACS_CONFIG_DIR=/opt/acs/etc
agentcronsystem start
```

The daemon loads config from `/opt/acs/etc/config.json` and stores data under `/opt/acs/data/`.

