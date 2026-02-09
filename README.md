# Agent Cron Scheduler (ACS)

A cross-platform cron scheduler daemon with a CLI and REST API. Manages scheduled jobs using standard 5-field cron expressions, captures output via piped I/O, and streams it in real time via Server-Sent Events (SSE).

Supports Windows, macOS, and Linux.

## Features

- **Cron scheduling** -- standard 5-field cron expressions with timezone support
- **REST API** -- full CRUD for jobs, paginated run history, real-time SSE streaming
- **CLI** -- manage jobs, view logs, trigger runs from the terminal
- **Cross-platform** -- Windows (Task Scheduler), macOS (launchd), Linux (systemd) service integration
- **Persistent storage** -- JSON-backed job store with atomic writes and corruption recovery
- **Run capture** -- stdout/stderr captured and stored per-run with automatic log rotation

## Installing a Production Release

Download the latest binary for your platform from the [Releases](https://github.com/Jtonna/agent-cron-scheduler/releases) page.

> One-liner install scripts have not been set up or tested yet. Please install manually from the Releases page for now.

After installing, verify with:

```sh
acs --version
```

## Quick Start

```sh
# Start the daemon (registers as a system service, runs in background)
acs start

# Add a job that runs every minute
acs add -n "hello" -s "* * * * *" -c "echo hello world"

# Trigger it immediately and follow the output
acs trigger hello --follow

# List all jobs
acs list

# Check daemon status
acs status

# View recent runs for a job
acs logs hello

# Stop the daemon
acs stop
```

The daemon starts an HTTP server on `127.0.0.1:8377`. Once running, you can also interact via the REST API:

```sh
# Health check
curl http://127.0.0.1:8377/health

# List jobs
curl http://127.0.0.1:8377/api/jobs

# Create a job
curl -X POST http://127.0.0.1:8377/api/jobs \
  -H "Content-Type: application/json" \
  -d '{"name":"curl-test","schedule":"* * * * *","execution":{"type":"ShellCommand","value":"echo from curl"}}'

# Stream events (SSE)
curl -N http://127.0.0.1:8377/api/events
```

---

## Developer Guide

### Installing a Development Environment

#### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (1.88+)
- [Node.js](https://nodejs.org/) 20+ (for frontend development only -- not required for `cargo build`)

#### Clone and Build

```sh
git clone https://github.com/Jtonna/agent-cron-scheduler.git
cd agent-cron-scheduler/acs

# Debug build
cargo build

# Release build
cargo build --release

# Or install directly to your PATH
cargo install --path .
```

The binary is at `acs/target/debug/acs` (or `acs/target/release/acs`).

`cargo build` does not build the frontend. The `web/` directory contains a static reference page embedded into the binary via `rust-embed`. The `build.rs` script verifies that `web/` exists but does not run npm or any frontend build step.

### Running in Development

```sh
# Foreground mode (logs print to the terminal)
cargo run -- start --foreground

# With verbose (debug-level) logging
cargo run -- start --foreground -v

# Custom port and data directory
cargo run -- start --foreground --port 9000 --data-dir /tmp/acs-dev
```

In a second terminal:

```sh
cargo run -- status
cargo run -- add -n "test" -s "* * * * *" -c "echo hello"
cargo run -- trigger test --follow
cargo run -- stop
```

### Frontend Development

The interactive Next.js dashboard in `frontend/` runs independently from the Rust binary (it is not embedded).

```sh
# Terminal 1: start the backend
cd acs && cargo run -- start --foreground

# Terminal 2: start the frontend dev server
cd frontend && npm run dev
# Open http://localhost:3000
```

The dev server proxies `/api/*` and `/health` to `http://127.0.0.1:8377` via rewrites in `next.config.ts`. The backend includes CORS middleware so direct cross-origin requests also work.

### Testing

```sh
# Full test suite
cargo test

# Specific modules
cargo test storage::
cargo test daemon::scheduler::

# Integration tests only
cargo test --test api_tests
cargo test --test cli_tests
cargo test --test scheduler_tests

# Lint and format checks
cargo clippy -- -D warnings
cargo fmt -- --check
```

### Project Structure

```
acs/                     # Rust project root
  src/
    main.rs              # Entry point, CLI dispatch
    models/              # Job, JobRun, DaemonConfig structs
    storage/             # JobStore + LogStore traits and implementations
    daemon/              # Daemon bootstrap, scheduler, executor, events, service registration
    server/              # Axum router, REST routes, SSE handler, health endpoint
    cli/                 # Clap CLI definition, subcommand handlers
    pty/                 # Process spawning abstraction
  web/                   # Static reference page (embedded via rust-embed)
  tests/                 # Integration tests (api, cli, scheduler)
frontend/                # Next.js interactive dashboard (independent)
docs/                    # Documentation
```

---

## Issue Tracking

ACS uses [beads](https://github.com/steveyegge/beads) for git-backed issue tracking. Issues live in `.beads/issues.jsonl` and are automatically synced to the [GitHub Projects kanban board](https://github.com/Jtonna/agent-cron-scheduler/projects) when merged to `main`. See [docs/beads-sync.md](docs/beads-sync.md) for details.

## Documentation

Full system documentation for developers and AI agents is available in the [docs/](docs/INDEX.md) directory.

## License

MIT
