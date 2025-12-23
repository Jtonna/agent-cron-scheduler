# Developer Guide

This document covers building, testing, and contributing to Agent Cron Scheduler.

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

## Running in Development

```sh
# Foreground mode (logs print to the terminal)
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

The daemon starts an HTTP server on `127.0.0.1:8377` by default. Open a second terminal to interact with it:

```sh
cargo run -- status
cargo run -- add -n "test" -s "* * * * *" -c "echo hello"
cargo run -- trigger test --follow
cargo run -- stop
```

## Testing

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

## Coverage

Coverage reports use [`cargo-llvm-cov`](https://github.com/taiki-e/cargo-llvm-cov):

```sh
# Install (one-time)
cargo install cargo-llvm-cov
rustup component add llvm-tools

# Run tests with coverage summary
cargo llvm-cov --tests --lib

# Generate HTML report
cargo llvm-cov --tests --lib --html
# Open target/llvm-cov/html/index.html
```

## Platform Notes

### Process spawning

The daemon uses piped I/O (`NoPtySpawner`) for process spawning, which runs child processes via `std::process::Command` with piped stdout/stderr. This reliably handles EOF on all platforms. Programs that check `isatty()` will detect non-TTY mode.

### Verbose logging

The `-v` flag enables debug-level tracing output. When running in foreground mode with `-v`, the daemon logs all scheduler ticks, job dispatches, executor events, and HTTP requests to the terminal.

### Configuration (config file only)

These fields are set via the config file and are not exposed as environment variables:

| Field | Description | Default |
|---|---|---|
| `max_log_file_size` | Max size per log file (bytes) | `10485760` (10 MB) |
| `pty_rows` | PTY terminal rows | `24` |
| `pty_cols` | PTY terminal columns | `80` |

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
    mod.rs             # PTY abstraction (NoPtySpawner, MockPtySpawner)
web/
  index.html           # Dashboard UI
  style.css            # Styles (dark/light theme)
  app.js               # Frontend logic (SSE, API calls)
tests/
  api_tests.rs         # HTTP API integration tests
  cli_tests.rs         # CLI integration tests
  scheduler_tests.rs   # End-to-end scheduler tests
```

## Architecture

See [ARCHITECTURE.md](ARCHITECTURE.md) for detailed design documentation.
