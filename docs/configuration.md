# ACS Configuration Guide

This document describes how to configure the Agent Cron Scheduler (ACS) daemon, including the config file format, resolution order, data directory locations, environment variables, and CLI global options.

## Config File Format

ACS uses a JSON configuration file. All fields are optional; any omitted field falls back to its built-in default value. An empty JSON object (`{}`) is a valid configuration that uses all defaults.

### Complete Example

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
  "pty_cols": 80
}
```

### Field Reference

| Field | Type | Default | Description |
|---|---|---|---|
| `host` | string | `"127.0.0.1"` | IP address the daemon HTTP server binds to. Use `"0.0.0.0"` to listen on all interfaces. |
| `port` | integer (u16) | `8377` | TCP port the daemon HTTP server listens on. |
| `data_dir` | string or null | `null` | Override the data directory path. When `null`, the platform default is used (see [Data Directory Locations](#data-directory-locations)). |
| `max_log_files_per_job` | integer | `50` | Maximum number of log files retained per job. Older logs are cleaned up automatically. |
| `max_log_file_size` | integer (bytes) | `10485760` (10 MB) | Maximum size in bytes for individual job run log files. **(Not currently enforced; reserved for future use.)** |
| `default_timeout_secs` | integer | `0` | Default timeout in seconds for job execution. A value of `0` means no timeout limit. |
| `broadcast_capacity` | integer | `4096` | Capacity of the internal broadcast channel used for job events (SSE streaming, log updates). |
| `pty_rows` | integer (u16) | `24` | Number of rows for the pseudo-terminal allocated to job processes. **(No effect; the production spawner uses piped I/O, not a PTY.)** |
| `pty_cols` | integer (u16) | `80` | Number of columns for the pseudo-terminal allocated to job processes. **(No effect; the production spawner uses piped I/O, not a PTY.)** |

### Partial Configuration

You only need to specify the fields you want to override. Unspecified fields use their defaults. For example, to only change the port and host:

```json
{
  "host": "0.0.0.0",
  "port": 9000
}
```

All other fields (`max_log_files_per_job`, `max_log_file_size`, etc.) will use their default values.

## Config File Resolution Order

When the daemon starts, it searches for a configuration file in the following order. The first file found is used. If no file is found at any location, built-in defaults are applied.

| Priority | Source | Path |
|---|---|---|
| 1 | `--config` CLI flag | Exact path provided via `acs start --config <path>`. If specified but the file does not exist, the daemon exits with an error. |
| 2 | `ACS_CONFIG_DIR` environment variable | `$ACS_CONFIG_DIR/config.json` |
| 3 | Platform config directory | `<platform_config_dir>/agent-cron-scheduler/config.json` (see platform paths below) |
| 4 | Data directory | `<data_dir>/config.json` |
| 5 | Built-in defaults | No file needed. Uses `DaemonConfig::default()` values as listed in the field reference above. |

### Platform Config Directories (Priority 3)

The platform config directory is resolved using the `dirs::config_dir()` function:

| Platform | Config Directory |
|---|---|
| Windows | `%APPDATA%\agent-cron-scheduler\config.json` |
| macOS | `~/Library/Application Support/agent-cron-scheduler/config.json` |
| Linux | `~/.config/agent-cron-scheduler/config.json` |

### Important Behavior

- Priority 1 (`--config`): If you explicitly pass a config file path and it does not exist, the daemon returns an error and does not start. This is the only priority level that fails on a missing file.
- Priorities 2-4: If the resolved path does not exist, the daemon silently moves to the next priority level.
- Priority 5: Always succeeds. The daemon runs with all default values.

## Data Directory Locations

The data directory stores jobs, run logs, scripts, the PID file, the port file, and the daemon log. It is resolved in the following order:

| Priority | Source | Description |
|---|---|---|
| 1 | `--data-dir` CLI flag | Explicit path passed to `acs start --data-dir <path>`. |
| 2 | `data_dir` field in config | The `data_dir` field in the loaded config file. |
| 3 | `ACS_DATA_DIR` environment variable | Override via environment variable. |
| 4 | Platform default | OS-specific default directory (see below). |

### Platform Default Data Directories

| Platform | Default Path | Notes |
|---|---|---|
| Windows | `%LOCALAPPDATA%\agent-cron-scheduler` | Per-user directory, no admin elevation required. Uses the `LOCALAPPDATA` environment variable. |
| macOS | `~/Library/Application Support/agent-cron-scheduler` | Resolved via `dirs::data_dir()`. Per-user directory. |
| Linux | `~/.local/share/agent-cron-scheduler` | Resolved via `dirs::data_dir()`. Per-user directory. |

On startup, the daemon ensures the data directory and its subdirectories (`logs/`, `scripts/`) exist. For the full data directory file layout, see [Storage](storage.md#1-data-directory-layout).

## Environment Variables

| Variable | Description |
|---|---|
| `ACS_DATA_DIR` | Override the data directory location. Takes effect when no `--data-dir` CLI flag and no `data_dir` config field is set. |
| `ACS_CONFIG_DIR` | Directory to search for `config.json`. Checked at priority 2 in the config resolution order, after the `--config` CLI flag but before platform and data directory fallbacks. |
| `RUST_LOG` | Controls the tracing/logging filter level for the **daemon process only** (not CLI client commands). Follows the `tracing_subscriber::EnvFilter` syntax. Examples: `info`, `debug`, `acs=debug,tower=warn`. Defaults to `info` if not set. **Important:** The `-v` flag initializes its own tracing subscriber before the daemon starts, so `RUST_LOG` is silently ignored when `-v` is present. Use one or the other, not both. |
| `LOCALAPPDATA` | (Windows only) Used to determine the default data directory. This variable is set automatically by Windows and should not normally need to be changed. |

## CLI Override Precedence

**Important:** There are two `--port` flags with different purposes:
- **Global `--port`** (e.g., `acs --port 9000 status`): Tells the CLI client which port to connect to when communicating with an already-running daemon. This does **not** affect which port the daemon listens on.
- **`acs start --port` (`-p`)** (e.g., `acs start -p 9000`): Sets the port the daemon binds to when starting.

When `acs start` is invoked, the daemon's listening port is resolved with the following precedence (highest to lowest):

1. `acs start --port` (`-p`) subcommand flag
2. `port` value in the loaded config file
3. Built-in default (`8377`)

For the bind host, the global `--host` flag overrides the config value only when it differs from the default (`127.0.0.1`).

For the full list of CLI options, see [CLI Reference](cli-reference.md).

## Examples

### Start with all defaults

```bash
acs start
```

The daemon binds to `127.0.0.1:8377` and stores data in the platform default directory.

### Start with a custom config file

```bash
acs start --config /etc/acs/config.json
```

### Start with a custom data directory and port

```bash
acs start --data-dir /var/lib/acs --port 9000
```

### Start in foreground mode for debugging

```bash
RUST_LOG=debug acs start --foreground
```

### Use environment variables for configuration

```bash
export ACS_DATA_DIR=/opt/acs/data
export ACS_CONFIG_DIR=/opt/acs/etc
acs start
```

The daemon loads config from `/opt/acs/etc/config.json` and stores data under `/opt/acs/data/`.

