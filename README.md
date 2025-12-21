# Agent Cron Scheduler (acs)

A cross-platform cron scheduler daemon with a CLI and web dashboard. Manages scheduled jobs using standard 5-field cron expressions, captures output via piped I/O, and streams it in real time via SSE.

Supports Windows, macOS, and Linux.

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (1.88+)

## Building

```sh
# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

The binary is named `acs` and will be at `target/debug/acs` (or `target/release/acs`).

## Running Tests

```sh
# Run the full test suite
cargo test

# Run tests with output visible
cargo test -- --nocapture

# Run a specific test module
cargo test storage::
cargo test daemon::scheduler::
cargo test cli::
cargo test server::

# Run integration tests only
cargo test --test api_tests
cargo test --test cli_tests
cargo test --test scheduler_tests

# Check for lint warnings
cargo clippy -- -D warnings

# Check formatting
cargo fmt -- --check
```

## Running Locally

### 1. Start the daemon

```sh
# Foreground mode (recommended for development -- logs print to the terminal)
cargo run -- start --foreground

# With verbose (debug-level) logging
cargo run -- start --foreground -v

# Custom port
cargo run -- start --foreground --port 9000

# Custom config file
cargo run -- start --foreground --config /path/to/config.json

# Custom data directory
cargo run -- start --foreground --data-dir /path/to/data
```

The daemon starts an HTTP server on `127.0.0.1:8377` by default.

### 2. Open the web dashboard

Once the daemon is running, open your browser to:

```
http://127.0.0.1:8377/
```

The web UI lets you add/edit/delete jobs, toggle enable/disable, trigger manual runs, and view live log output.

### 3. Use the CLI

Open a second terminal to interact with the running daemon:

```sh
# Check daemon status
cargo run -- status

# Add a job that runs every minute
cargo run -- add -n "hello" -s "* * * * *" -c "echo hello world"

# Add a job that runs every 5 minutes
cargo run -- add -n "date-check" -s "*/5 * * * *" -c "date"

# Add a job with a timezone
cargo run -- add -n "ny-morning" -s "0 9 * * *" -c "echo good morning" --timezone "America/New_York"

# Add a job with environment variables
cargo run -- add -n "env-test" -s "*/10 * * * *" -c "echo $MY_VAR" -e MY_VAR=hello -e OTHER=world

# Add a job that runs a script file
cargo run -- add -n "deploy" -s "0 2 * * *" --script deploy.sh

# Add a job that logs the full environment before each run (useful for debugging)
cargo run -- add -n "debug-job" -s "*/5 * * * *" -c "echo hello" --log-env

# Add a job in disabled state
cargo run -- add -n "paused-job" -s "0 * * * *" -c "echo paused" --disabled

# List all jobs
cargo run -- list

# List as JSON
cargo run -- list --json

# List only enabled jobs
cargo run -- list --enabled

# List only disabled jobs
cargo run -- list --disabled

# Manually trigger a job (don't wait for the cron schedule)
cargo run -- trigger hello

# Trigger and follow output live
cargo run -- trigger hello --follow

# View recent runs for a job
cargo run -- logs hello

# View last 5 runs
cargo run -- logs hello --last 5

# View a specific run's log (use the run ID from the list)
cargo run -- logs hello --run <RUN_UUID>

# Follow live output for a job
cargo run -- logs hello --follow

# Show last N lines of a run's log
cargo run -- logs hello --tail 50

# Enable/disable a job
cargo run -- disable hello
cargo run -- enable hello

# Remove a job
cargo run -- remove hello

# Remove without confirmation prompt
cargo run -- remove hello --yes

# Stop the daemon
cargo run -- stop

# Force-kill the daemon
cargo run -- stop --force

# Remove system service registration
cargo run -- uninstall

# Remove service and all data
cargo run -- uninstall --purge
```

### 4. REST API

The daemon exposes a REST API you can hit directly:

```sh
# Health check
curl http://127.0.0.1:8377/health

# List jobs
curl http://127.0.0.1:8377/api/jobs

# List enabled jobs only
curl http://127.0.0.1:8377/api/jobs?enabled=true

# Create a job
curl -X POST http://127.0.0.1:8377/api/jobs \
  -H "Content-Type: application/json" \
  -d '{"name":"curl-test","schedule":"* * * * *","execution":{"type":"ShellCommand","value":"echo from curl"}}'

# Create a job with environment logging enabled (dumps all env vars before each run)
curl -X POST http://127.0.0.1:8377/api/jobs \
  -H "Content-Type: application/json" \
  -d '{"name":"debug-test","schedule":"* * * * *","execution":{"type":"ShellCommand","value":"echo hello"},"log_environment":true}'

# Get a job by name or UUID
curl http://127.0.0.1:8377/api/jobs/curl-test

# Update a job
curl -X PATCH http://127.0.0.1:8377/api/jobs/curl-test \
  -H "Content-Type: application/json" \
  -d '{"schedule":"*/5 * * * *"}'

# Enable/disable a job
curl -X POST http://127.0.0.1:8377/api/jobs/curl-test/enable
curl -X POST http://127.0.0.1:8377/api/jobs/curl-test/disable

# Delete a job
curl -X DELETE http://127.0.0.1:8377/api/jobs/curl-test

# Trigger a job
curl -X POST http://127.0.0.1:8377/api/jobs/curl-test/trigger

# List runs (with pagination)
curl http://127.0.0.1:8377/api/jobs/curl-test/runs?limit=20&offset=0

# Get a specific run's log output
curl http://127.0.0.1:8377/api/runs/<RUN_UUID>/log

# Stream events (SSE)
curl -N http://127.0.0.1:8377/api/events

# Service status
curl http://127.0.0.1:8377/api/service/status

# Shutdown
curl -X POST http://127.0.0.1:8377/api/shutdown
```

## Configuration

Configuration is resolved in this order (highest priority first):

1. CLI flags (`--port 9000`)
2. Environment variables (`ACS_PORT=9000`)
3. Config file (`config.json` in platform config directory)
4. Compiled defaults

| Variable | Description | Default |
|---|---|---|
| `ACS_HOST` | Bind address | `127.0.0.1` |
| `ACS_PORT` | HTTP port | `8377` |
| `ACS_DATA_DIR` | Data directory | Platform default |
| `ACS_CONFIG_DIR` | Config directory | Platform default |
| `ACS_MAX_LOG_FILES` | Max log files per job | `50` |
| `ACS_TIMEOUT` | Default job timeout (0 = none) | `0` |
| `ACS_BROADCAST_CAPACITY` | SSE broadcast channel size | `4096` |

Additional config fields (set via config file only):

| Field | Description | Default |
|---|---|---|
| `max_log_file_size` | Max size per log file (bytes) | `10485760` (10 MB) |
| `pty_rows` | PTY terminal rows | `24` |
| `pty_cols` | PTY terminal columns | `80` |

See `config.example.json` for a template.

## Platform Notes

### Process spawning

The daemon uses `NoPtySpawner` by default, which spawns child processes via `std::process::Command` with piped stdout/stderr. This is used instead of the `RealPtySpawner` (ConPTY on Windows, forkpty on Unix) because Windows ConPTY has a known issue where the master-side reader does not receive EOF after the child process exits, causing job runs to hang in "Running" status. The piped I/O approach reliably handles EOF on all platforms.

If full PTY emulation is needed (for programs that require a terminal environment), the `RealPtySpawner` implementation exists in `src/pty/mod.rs` but is not used by the daemon by default.

### Verbose logging

The `-v` flag enables debug-level tracing output. When running in foreground mode with `-v`, the daemon logs all scheduler ticks, job dispatches, executor events, and HTTP requests to the terminal.

## Cron Schedule Format

Standard 5-field cron expressions:

```
* * * * *
| | | | |
| | | | +-- Day of week (0-7, Sun=0 or 7)
| | | +---- Month (1-12)
| | +------ Day of month (1-31)
| +-------- Hour (0-23)
+---------- Minute (0-59)
```

Examples:
- `* * * * *` -- every minute
- `*/5 * * * *` -- every 5 minutes
- `0 * * * *` -- every hour
- `0 9 * * 1-5` -- 9 AM weekdays
- `0 0 1 * *` -- midnight on the 1st of each month

## Project Structure

```
src/
  main.rs              # Entry point, CLI dispatch
  lib.rs               # Module declarations
  errors.rs            # Error types
  models/
    mod.rs             # Re-exports
    job.rs             # Job, NewJob, JobUpdate, ExecutionType
    run.rs             # JobRun, RunStatus
    config.rs          # DaemonConfig
  storage/
    mod.rs             # Storage traits (JobStore, LogStore)
    jobs.rs            # JSON file persistence for jobs
    logs.rs            # Per-run log file management
  daemon/
    mod.rs             # Daemon bootstrap, PID file, config loading, shutdown
    scheduler.rs       # Cron tick engine
    executor.rs        # Process spawning, output capture
    events.rs          # JobEvent enum, broadcast channel
    service.rs         # Platform service install/uninstall
  server/
    mod.rs             # Axum router, AppState
    routes.rs          # REST endpoint handlers
    sse.rs             # SSE streaming handler
    health.rs          # Health check endpoint
  cli/
    mod.rs             # CLI definition (clap), command dispatch
    jobs.rs            # add, remove, list, enable, disable, trigger
    logs.rs            # logs --follow, --run, --last, --tail
    daemon.rs          # start, stop, status, uninstall
  pty/
    mod.rs             # PTY abstraction (RealPtySpawner, NoPtySpawner, MockPtySpawner)
web/
  index.html           # Dashboard UI
  style.css            # Styles (dark/light theme)
  app.js               # Frontend logic (SSE, API calls)
tests/
  api_tests.rs         # HTTP API integration tests
  cli_tests.rs         # CLI integration tests
  scheduler_tests.rs   # End-to-end scheduler tests
```

## License

MIT
