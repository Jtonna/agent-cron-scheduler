# Agent Cron Scheduler (ACS)

A cross-platform multi-step workflow scheduler daemon with a CLI, REST API, and built-in web UI. Workflows run on standard cron schedules and chain together shell commands, scripts, HTTP calls, branching logic, and AI-agent invocations -- with live output streaming over Server-Sent Events.

Supports Windows, macOS, and Linux.

## Features

- **Multi-step workflows** -- chain `shell`, `script`, `http`, `match`, `set_var`, and `agent` steps with per-step failure policies, retries, timeouts, and stdin/env routing
- **Cron scheduling** -- 5- or 6-field expressions with IANA timezone support; `Cron` or `WaitForCompletion` schedule modes
- **REST API + OpenAPI** -- full workflow CRUD, paginated run history, per-step log slicing, live SSE event stream; Swagger UI served at `http://127.0.0.1:8377/`
- **CLI** -- `agentcronsystem workflows {create,list,get,update,delete,trigger,runs}` plus daemon lifecycle commands
- **Cross-platform service integration** -- Windows (Registry Run key), macOS (launchd), Linux (systemd user unit)
- **SQLite-backed storage** -- workflow definitions and run history persisted to `acs.db` (WAL mode); combined per-run log files with per-step byte-offset indexing
- **Agent steps** -- first-class `claude_code_cli` step kind for AI-assisted workflows

## Installing a Production Release

Download the latest binary for your platform from the [Releases](https://github.com/Jtonna/agent-cron-scheduler/releases) page.

> One-liner install scripts have not been set up or tested yet. Please install manually from the Releases page for now.

After installing, verify with:

```sh
agentcronsystem --version
```

To update an existing installation:

```sh
agentcronsystem update
```

## Quick Start

```sh
# Start the daemon (registers as a system service, runs in background)
agentcronsystem start

# Create a workflow from a JSON file
agentcronsystem workflows create --file hello.json

# List workflows
agentcronsystem workflows list

# Trigger a workflow manually (by name or UUID)
agentcronsystem workflows trigger hello

# List recent runs for a workflow
agentcronsystem workflows runs hello

# Check daemon status
agentcronsystem status

# Stop the daemon
agentcronsystem stop
```

A minimal `hello.json`:

```json
{
  "name": "hello",
  "schedule": "* * * * *",
  "steps": [
    { "kind": "shell", "id": "say-hi", "command": "echo hello world" }
  ]
}
```

The daemon serves an HTTP API on `127.0.0.1:8377`. The same operations via curl:

```sh
# Health check
curl http://127.0.0.1:8377/health

# List workflows
curl http://127.0.0.1:8377/api/workflows

# Create a workflow
curl -X POST http://127.0.0.1:8377/api/workflows \
  -H "Content-Type: application/json" \
  -d '{
    "name": "curl-test",
    "schedule": "* * * * *",
    "steps": [
      { "kind": "shell", "id": "step1", "command": "echo from curl" }
    ]
  }'

# Trigger a workflow (returns 202 + run_id)
curl -X POST http://127.0.0.1:8377/api/workflows/curl-test/trigger \
  -H "Content-Type: application/json" \
  -d '{}'

# Subscribe to the live event stream (SSE)
curl -N http://127.0.0.1:8377/api/events/workflows
```

Open `http://127.0.0.1:8377/` in a browser for the embedded Swagger UI.

---

## Developer Guide

### Prerequisites

- [Rust](https://rustup.rs/) stable toolchain (1.88+)
- [Node.js](https://nodejs.org/) 20+ (only required for frontend development; not needed for `cargo build`)

### Clone and Build

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

The binary is at `acs/target/debug/agentcronsystem` (or `acs/target/release/agentcronsystem`).

`cargo build` does not build the frontend. The `web/` directory contains Swagger UI assets and the `openapi.yaml` spec, embedded into the binary via `rust-embed`. The `build.rs` script verifies that `web/` exists; it does not run npm.

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
cargo run -- workflows create --file ./hello.json
cargo run -- workflows trigger hello
cargo run -- stop
```

### Frontend Development

The interactive Next.js dashboard in `electron/packages/frontend/` runs independently from the Rust binary (it is not embedded).

```sh
# Terminal 1: start the backend
cd acs && cargo run -- start --foreground

# Terminal 2: start the frontend dev server
cd electron/packages/frontend && npm run dev
# Open http://localhost:3000 (Next.js default port; may vary if 3000 is in use)
```

The dev server proxies `/api/*` and `/health` to `http://127.0.0.1:8377` via rewrites in `next.config.ts`. The backend ships with CORS middleware so direct cross-origin requests also work.

### Testing

```sh
# Full test suite
cargo test

# Specific modules
cargo test storage::
cargo test daemon::scheduler::

# Integration tests
cargo test --test workflow_api_tests
cargo test --test cli_tests
cargo test --test migration_tests

# Lint and format checks
cargo clippy -- -D warnings
cargo fmt -- --check
```

### Project Structure

```
acs/                     # Rust project root
  src/
    main.rs              # Entry point, CLI dispatch
    lib.rs               # Re-exports all public modules
    errors.rs            # Custom error types (AcsError)
    models/
      workflow.rs        # Workflow, NewWorkflow, StepDef, WorkflowRun, TriggerParams, etc.
      config.rs          # DaemonConfig
    migrations/          # ACS's migrations + registry (run via the milepost framework)
    storage/             # WorkflowStore + WorkflowRunStore (SQLite) + log sinks
    daemon/              # Daemon bootstrap, scheduler, workflow executor, events, service registration
    server/              # Axum router, REST routes, SSE handler, health endpoint
    cli/                 # Clap CLI definitions and subcommand handlers
    workflow/            # Multi-step execution engine: executor, steps, agents, template, log_sink, finalize
    pty/                 # Process spawning abstraction
  web/                   # Swagger UI + openapi.yaml (embedded via rust-embed)
  tests/                 # Integration tests (workflow_api_tests, cli_tests, migration_tests)
milepost/                # Generic SQLite migration framework (library; knows nothing about ACS)
electron/                # Electron app and frontend
  packages/
    frontend/            # Next.js dashboard (independent)
docs/                    # Markdown documentation -- start at docs/INDEX.md
```

---

## AI-Assisted Development

This project uses [starterpack](https://github.com/Jtonna/starterpack) to augment AI agent workflows for writing code, documentation, testing, and lifecycle management, including GitHub issue tracking via [beads](https://github.com/steveyegge/beads).

## Documentation

Full system documentation lives in [docs/INDEX.md](docs/INDEX.md). Highlights:

- [Architecture](docs/architecture.md) -- modules, data flow, concurrency model
- [CLI Reference](docs/cli-reference.md) -- every `agentcronsystem` subcommand
- [API Reference](docs/api-reference.md) -- REST endpoints, SSE events, schemas
- [Workflow Management](docs/workflow-management.md) -- step kinds, template substitution, failure policies
- [Configuration](docs/configuration.md) -- config file format and data directories
- [Storage](docs/storage.md) -- SQLite layout, log sinks, migrations (`milepost` framework + `acs/src/migrations/`)
- [Service Registration](docs/service-registration.md) -- platform-specific service setup
- [Troubleshooting](docs/troubleshooting.md) -- common problems and fixes

## License

MIT
