# Developer Guide

This document covers building, testing, and contributing to Agent Cron Scheduler.

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (1.88+)
- [Node.js](https://nodejs.org/) 20+ (for frontend development only -- not required for `cargo build`)

## Building

All cargo commands are run from the `acs/` directory:

```sh
cd acs

# Debug build
cargo build

# Release build (optimized)
cargo build --release
```

The binary is named `acs` and will be at `acs/target/debug/acs` (or `acs/target/release/acs`).

`cargo build` does not build the frontend. The `web/` directory contains a static
API and CLI reference page (plain HTML/CSS) that is embedded into the binary via
`rust-embed`. The `build.rs` script verifies that `web/` exists but does not run
npm or any frontend build step.

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

### Frontend Development

The interactive Next.js dashboard in `frontend/` is developed independently from
the Rust binary. It is not embedded into the binary -- the embedded `web/`
content is a static API and CLI reference page.

Run the backend and frontend in separate terminals:

```sh
# Terminal 1: start the backend (from acs/ directory)
cd acs
cargo run -- start --foreground

# Terminal 2: start the Next.js dev server
cd frontend
npm run dev
# Open http://localhost:3000
```

**How it works:**

- The frontend API client (`frontend/src/lib/api.ts`) reads
  `NEXT_PUBLIC_API_URL` to determine the API base URL. When empty (the default),
  it uses relative paths (e.g., `/api/jobs`).
- During `npm run dev`, `next.config.ts` configures rewrites that proxy `/api/*`
  and `/health` to `http://127.0.0.1:8377`, so relative-path API calls reach the
  backend automatically.
- The backend includes CORS middleware (`CorsLayer` with `allow_origin(Any)`) so
  cross-origin requests from `localhost:3000` to `127.0.0.1:8377` are permitted.
  This is safe because the daemon binds to localhost only.
- Alternatively, set `NEXT_PUBLIC_API_URL=http://127.0.0.1:8377` to bypass
  rewrites and have the frontend call the backend directly (requires CORS).

**Two API client modes:**

| Mode | `NEXT_PUBLIC_API_URL` | Proxy | CORS needed |
|---|---|---|---|
| Dev server with rewrites | `""` (empty/unset) | Yes -- next.config.ts rewrites | No |
| Dev server direct | `http://127.0.0.1:8377` | No | Yes |

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
# Open acs/target/llvm-cov/html/index.html
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
acs/                     # Rust project root
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
      daemon.rs          # start, stop, restart, status, uninstall
    pty/
      mod.rs             # PTY abstraction (NoPtySpawner, MockPtySpawner)
  web/                   # Static API & CLI reference page (embedded into binary)
  tests/
    api_tests.rs         # HTTP API integration tests
    cli_tests.rs         # CLI integration tests
    scheduler_tests.rs   # End-to-end scheduler tests
frontend/                # Next.js interactive dashboard (runs independently)
  src/app/             # App Router pages and layouts
  next.config.ts       # Static export configuration
  package.json         # Frontend dependencies
docs/                    # Documentation
  ARCHITECTURE.md        # Detailed design documentation
  DEVIATIONS_FROM_SPEC.md
  SPEC.md                # Original design specification
```

## Architecture

See [ARCHITECTURE.md](docs/ARCHITECTURE.md) for detailed design documentation.
