# Agent Cron Scheduler (acs)

A cross-platform cron scheduler daemon with a CLI and REST API. Manages scheduled jobs using standard 5-field cron expressions, captures output via piped I/O, and streams it in real time via SSE.

Supports Windows, macOS, and Linux.

## Installation

```sh
# Build from source
cargo install --path .
```

The binary `acs` will be installed to your Cargo bin directory (typically `~/.cargo/bin/`).

## Quick Start

```sh
# Start the daemon (registers as a system service and runs in background)
acs start

# View the API & CLI reference page
# http://127.0.0.1:8377/

# Add a job that runs every minute
acs add -n "hello" -s "* * * * *" -c "echo hello world"

# Trigger it immediately and follow the output
acs trigger hello --follow

# Check daemon status
acs status
```

## CLI Reference

### Daemon Management

```sh
# Start the daemon (background, auto-registers as system service)
acs start

# Start in foreground (logs to terminal)
acs start --foreground

# Start with verbose logging
acs start --foreground -v

# Start on a custom port
acs start --port 9000

# Start with a custom config file
acs start --config /path/to/config.json

# Start with a custom data directory
acs start --data-dir /path/to/data

# Check daemon status
acs status

# Restart the daemon
acs restart

# Stop the daemon gracefully
acs stop

# Force-kill the daemon
acs stop --force

# Remove system service registration
acs uninstall

# Remove service and delete all data
acs uninstall --purge
```

### Managing Jobs

```sh
# Add a job with a cron schedule
acs add -n "hello" -s "* * * * *" -c "echo hello world"

# Add a job that runs every 5 minutes
acs add -n "date-check" -s "*/5 * * * *" -c "date"

# Add a job with a timezone
acs add -n "ny-morning" -s "0 9 * * *" -c "echo good morning" --timezone "America/New_York"

# Add a job with environment variables
acs add -n "env-test" -s "*/10 * * * *" -c "echo $MY_VAR" -e MY_VAR=hello -e OTHER=world

# Add a job that runs a script file
acs add -n "deploy" -s "0 2 * * *" --script deploy.sh

# Add a job that logs the full environment before each run
acs add -n "debug-job" -s "*/5 * * * *" -c "echo hello" --log-env

# Add a job in disabled state
acs add -n "paused-job" -s "0 * * * *" -c "echo paused" --disabled

# List all jobs
acs list

# List as JSON
acs list --json

# List only enabled/disabled jobs
acs list --enabled
acs list --disabled

# Enable/disable a job
acs enable hello
acs disable hello

# Remove a job
acs remove hello

# Remove without confirmation prompt
acs remove hello --yes
```

### Triggering and Logs

```sh
# Manually trigger a job
acs trigger hello

# Trigger and follow output live
acs trigger hello --follow

# View recent runs for a job
acs logs hello

# View last 5 runs
acs logs hello --last 5

# View a specific run's log
acs logs hello --run <RUN_UUID>

# Follow live output for a job
acs logs hello --follow

# Show last N lines of a run's log
acs logs hello --tail 50
```

### Global Options

```sh
# Connect to a daemon on a different host/port
acs --host 192.168.1.10 --port 9000 list

# Verbose output
acs -v status
```

## Daemon Logs

The daemon writes logs to `daemon.log` inside the data directory:

- **Windows**: `%LOCALAPPDATA%\agent-cron-scheduler\daemon.log`
- **macOS/Linux**: `~/.local/share/agent-cron-scheduler/daemon.log`
- **Custom**: set `ACS_DATA_DIR` or use `--data-dir` to change the location.

The log file is truncated on each daemon startup so every session begins fresh. During operation, if the file grows beyond 1 GB the oldest 25% is automatically dropped, keeping the newest 75%. You can also view recent daemon logs via the API:

```sh
curl http://127.0.0.1:8377/api/logs?tail=200
```

## Embedded Reference Page

Once the daemon is running, open your browser to:

```
http://127.0.0.1:8377/
```

The embedded page is a static API and CLI reference -- a quick-reference guide
to available endpoints and CLI commands. It is not an interactive dashboard.

### Interactive Frontend (Optional)

An interactive Next.js dashboard lives in `frontend/` and runs independently
from the daemon. It is not embedded into the binary.

```sh
# Terminal 1: start the backend daemon
cargo run -- start --foreground

# Terminal 2: start the frontend dev server
cd frontend
npm run dev
# Open http://localhost:3000
```

The frontend dev server runs on `localhost:3000` and proxies API requests to the
backend on `127.0.0.1:8377` via rewrites configured in `next.config.ts`.

**Port discovery:** The daemon writes an `acs.port` file (alongside `acs.pid`)
in the data directory containing the port number it is listening on. The frontend
or external tooling can read this file to discover the backend port
automatically.

## REST API

The daemon exposes a REST API:

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

# Create a job with environment logging enabled
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

# Daemon logs
curl http://127.0.0.1:8377/api/logs

# Daemon logs (last 100 lines)
curl http://127.0.0.1:8377/api/logs?tail=100

# Restart the daemon
curl -X POST http://127.0.0.1:8377/api/restart

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

See `config.example.json` for a template.

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

## License

MIT
