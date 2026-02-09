# ACS Enhancements

## ENH-001: Dynamic Trigger Parameters

### Status

Not currently supported. Requires implementation.

### Problem

The trigger endpoint (`POST /api/jobs/{id}/trigger`) accepts no request body. The shell command, environment variables, and working directory are all fixed at job creation time. This means every trigger of a job runs the exact same command with the exact same configuration.

This prevents ACS from being used as a **real-time chat backend** where an external service (web UI, Telegram bot, Discord bot, etc.) needs to pass a user's message into an agent job at trigger time.

### Current Behavior

```
POST /api/jobs/{id}/trigger
# No body accepted
# Returns 202 immediately
# Runs the exact command defined at job creation
```

The job's command is baked in:

```json
{
  "name": "realestate-agent",
  "schedule": "disabled",
  "execution": {
    "type": "ShellCommand",
    "value": "claude -p \"fixed prompt here\" --session-id abc123 --output-format stream-json"
  }
}
```

There is no way to change `"fixed prompt here"` per trigger.

### Proposed Enhancement

Add an optional request body to the trigger endpoint that supports:

1. **`args`** — additional arguments appended to the shell command
2. **`env`** — environment variables merged into the job's existing env_vars for this run only
3. **`input`** — string piped to the process's stdin instead of appended to the command

These are per-trigger overrides. They do not modify the job definition.

### Proposed API

```
POST /api/jobs/{id}/trigger
Content-Type: application/json

{
  "args": "--resume 550e8400-e29b-41d4-a716-446655440000 -p \"what listings are available in Miami?\"",
  "env": {
    "USER_ID": "telegram:12345",
    "CHANNEL": "telegram"
  },
  "input": null
}
```

All fields are optional. An empty body or no body at all preserves current behavior (backwards compatible).

### Proposed Response

The trigger response should include the `run_id` so the caller can immediately subscribe to SSE for that specific run:

```json
{
  "message": "Job triggered",
  "job_id": "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee",
  "job_name": "realestate-agent",
  "run_id": "ffffffff-1111-2222-3333-444444444444"
}
```

The current response does not include `run_id`, which forces callers to guess which run belongs to their trigger when multiple triggers happen concurrently.

### Implementation Notes

#### `args` behavior

The `args` string is appended to the existing command before shell execution:

```
# Job definition:
execution.value = "claude -p"

# Trigger with args:
args = "--resume UUID -p \"user message\""

# Resulting command:
cmd.exe /C "claude -p --resume UUID -p \"user message\""    # Windows
/bin/sh -c "claude -p --resume UUID -p \"user message\""    # Unix
```

#### `env` behavior

Trigger-time env vars are merged on top of the job's `env_vars` (if any), which are merged on top of the inherited environment. Trigger env vars take highest precedence for this run only. The job definition is not modified.

```
Inherited env  <  job.env_vars  <  trigger.env
```

#### `input` behavior

If `input` is provided, it is written to the spawned process's stdin after launch. This is an alternative to `args` for cases where the payload is large or contains characters that are difficult to shell-escape.

#### `run_id` in response

The executor currently generates the `run_id` inside `spawn_job()` after the trigger endpoint has already returned 202. To include `run_id` in the response, either:

- **Option A:** Pre-generate the `run_id` in the trigger handler and pass it to the executor via the dispatch channel (cleanest).
- **Option B:** Make the trigger handler wait for the executor to acknowledge the job and return the `run_id` (adds latency but guarantees accuracy).

Option A is recommended.

#### Dispatch channel change

The dispatch channel currently sends `Job` structs. This would need to change to a wrapper:

```rust
struct DispatchRequest {
    job: Job,
    run_id: Uuid,           // pre-generated
    args: Option<String>,
    env: Option<HashMap<String, String>>,
    input: Option<String>,
}
```

#### Command builder change

`build_command()` in `executor.rs` currently only receives a `&Job`. It would need to accept the optional `args` and `env` from the `DispatchRequest`:

```rust
fn build_command(job: &Job, trigger_args: Option<&str>, trigger_env: Option<&HashMap<String, String>>) -> CommandBuilder
```

### Usage: Chat Router Integration

This enhancement enables a **Chat Router** service to use ACS as its agent execution backend.

#### Setup: Create a template job per product (one-time)

```sh
acs add \
  -n "realestate-chat" \
  --schedule "disabled" \
  --command "claude -p" \
  --env "CLAUDE_MODEL=sonnet" \
  --env "MCP_SERVERS=crm-server" \
  --timeout 120 \
  --working-dir "/opt/realestate-wizard"
```

The job is created with `schedule: "disabled"` so it never runs on its own — it only runs when triggered.

#### Runtime: Chat Router triggers per user message

```sh
# First message — new session
curl -X POST http://127.0.0.1:8377/api/jobs/realestate-chat/trigger \
  -H "Content-Type: application/json" \
  -d '{
    "args": "--session-id 550e8400-e29b-41d4-a716-446655440000 --output-format stream-json --dangerously-skip-permissions -p \"what homes are available in Miami under 500k?\"",
    "env": {
      "USER_ID": "telegram:12345",
      "SESSION_ORIGIN": "telegram"
    }
  }'

# Response:
# { "run_id": "ffffffff-1111-2222-3333-444444444444", ... }

# Follow-up message — resume session
curl -X POST http://127.0.0.1:8377/api/jobs/realestate-chat/trigger \
  -H "Content-Type: application/json" \
  -d '{
    "args": "--resume 550e8400-e29b-41d4-a716-446655440000 --output-format stream-json --dangerously-skip-permissions -p \"show me the ones with a pool\"",
    "env": {
      "USER_ID": "telegram:12345",
      "SESSION_ORIGIN": "telegram"
    }
  }'
```

#### Runtime: Chat Router subscribes to SSE for the response

```sh
# Using the run_id from the trigger response
curl -N "http://127.0.0.1:8377/api/events?job_id=JOB_UUID&run_id=RUN_UUID"
```

The Chat Router listens for `Output` events (streaming text) and `Completed`/`Failed` events (end of response), then forwards the result back to Telegram/Discord/web UI.

#### Full flow

```
User (Telegram)
    |
    v
Telegram Daemon
    |
    v
Chat Router
    |  1. POST /api/jobs/realestate-chat/trigger  { args: "--resume UUID -p \"user msg\"" }
    |  2. GET  /api/events?run_id=RUN_UUID         (SSE stream)
    v
ACS Daemon
    |  3. Spawns: /bin/sh -c "claude -p --resume UUID -p \"user msg\" --output-format stream-json"
    |  4. Streams Output events via SSE
    v
Chat Router
    |  5. Collects streamed output, formats response
    v
Telegram Daemon
    |  6. Sends response to user
    v
User (Telegram)
```

### What This Gets Us

- **ACS becomes a general-purpose agent execution backend**, not just a cron scheduler
- **Every agent run is logged** with full stdout/stderr capture, exit codes, and timestamps — for free
- **Chat sessions are traceable** through ACS's existing run history and log storage
- **The Telegram daemon stays thin** — it only translates between Telegram API and the Chat Router's HTTP API
- **The Chat Router stays thin** — it manages sessions and routes to ACS, which handles the heavy lifting
- **No new execution infrastructure** — reuses ACS's existing process spawning, PTY, timeout handling, and event broadcasting

### Security Considerations

- The `args` field allows arbitrary command injection if not handled carefully. Since ACS already runs arbitrary shell commands by design (the job's command is user-defined), this does not introduce a new attack surface — but it should be documented that ACS is a trusted internal service and must not be exposed to untrusted networks.
- The `env` field should not allow overriding security-sensitive variables like `PATH` or `HOME` unless explicitly permitted in configuration.
- The `input` field (stdin) is safer than `args` for passing untrusted user content since it avoids shell interpretation.

### Priority

High. This is the prerequisite for using ACS as the execution backend for the Chat Router, which is needed for Telegram/Discord/web chat integration across both VantageFeed and RealestateWizard.
