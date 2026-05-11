# ACS REST API Reference

This document provides a comprehensive reference for every endpoint exposed by the Agent Cron Scheduler (ACS) HTTP server.

Base URL: `http://127.0.0.1:8377` (default port; see [Configuration](configuration.md) for how to change it)

All request and response bodies use JSON (`Content-Type: application/json`) unless otherwise noted.

---

## Table of Contents

- [Conventions](#conventions)
- [Error Response Format](#error-response-format)
- [Workflow Identifier Resolution](#workflow-identifier-resolution)
- [Endpoints](#endpoints)
  - [GET /health](#get-health)
  - [GET /api/workflows](#get-apiworkflows)
  - [POST /api/workflows](#post-apiworkflows)
  - [GET /api/workflows/{id}](#get-apiworkflowsid)
  - [PATCH /api/workflows/{id}](#patch-apiworkflowsid)
  - [DELETE /api/workflows/{id}](#delete-apiworkflowsid)
  - [POST /api/workflows/{id}/trigger](#post-apiworkflowsidtrigger)
  - [GET /api/workflows/{id}/runs](#get-apiworkflowsidruns)
  - [GET /api/runs/{run_id}](#get-apirunsrun_id)
  - [POST /api/runs/{run_id}/kill](#post-apirunsrun_idkill)
  - [GET /api/runs/{run_id}/log](#get-apirunsrun_idlog)
  - [GET /api/events/workflows](#get-apieventsworkflows)
  - [POST /api/shutdown](#post-apishutdown)
  - [POST /api/restart](#post-apirestart)
  - [GET /api/logs](#get-apilogs)
- [Data Models](#data-models)
  - [Workflow](#workflow)
  - [NewWorkflow](#newworkflow)
  - [WorkflowUpdate](#workflowupdate)
  - [StepDef](#stepdef)
  - [StepDefCommon](#stepdefcommon)
  - [FailurePolicy](#failurepolicy)
  - [TriggerParams](#triggerparams)
  - [WorkflowRun](#workflowrun)
  - [StepRun](#steprun)
  - [RunStatus](#runstatus)
- [SSE Event Types](#sse-event-types)
- [Validation Rules](#validation-rules)

---

## Conventions

- All timestamps use ISO 8601 format in UTC (e.g., `"2025-01-15T10:30:00Z"`).
- UUIDs are v7 (time-ordered), serialized as standard hyphenated strings (e.g., `"01941234-5678-7abc-def0-123456789abc"`).
- CORS is fully open: all origins, methods, and headers are allowed.

---

## Error Response Format

All error responses share a consistent JSON structure:

```json
{
  "error": "<error_code>",
  "message": "<human-readable description>"
}
```

### Error Codes

| `error` value       | Typical HTTP Status | Description                                        |
|----------------------|---------------------|----------------------------------------------------|
| `not_found`          | 404                 | The requested resource does not exist              |
| `validation_error`   | 422                 | Request body or parameters failed validation       |
| `conflict`           | 409                 | A resource with the same unique key already exists |
| `internal_error`     | 500                 | An unexpected server-side error occurred           |
| `bad_request`        | 400                 | Malformed request (e.g., invalid UUID path param)  |

---

## Workflow Identifier Resolution

All endpoints that accept an `{id}` path parameter support two lookup strategies:

1. **UUID** -- If the value parses as a valid UUID, the workflow is looked up by its `id` field.
2. **Name** -- If UUID parsing fails, the value is treated as a workflow name (slug) and looked up via `find_by_name`.

This means you can use either `GET /api/workflows/01941234-5678-7abc-def0-123456789abc` or `GET /api/workflows/my-pipeline` interchangeably.

If neither lookup finds a matching workflow, a `404 not_found` error is returned.

---

## Endpoints

### GET /health

Returns daemon health status, including uptime, workflow counts, version, and platform service registration.

**Request:** No body, no query parameters.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Health check succeeded |

```json
{
  "status": "ok",
  "uptime_seconds": 3600,
  "active_jobs": 5,
  "total_jobs": 8,
  "version": "4.2.5",
  "data_dir": "/home/user/.local/share/agent-cron-scheduler",
  "service": {
    "registered": true,
    "platform": "linux",
    "details": "/home/user/.config/systemd/user/agentcronsystem.service"
  }
}
```

| Field            | Type    | Description                                                                               |
|------------------|---------|-------------------------------------------------------------------------------------------|
| `status`         | string  | Always `"ok"` when the server is responsive                                               |
| `uptime_seconds` | integer | Seconds since the daemon process started                                                  |
| `active_jobs`    | integer | Number of enabled workflows (field name is legacy; reflects enabled workflow count)       |
| `total_jobs`     | integer | Total number of workflows (field name is legacy; reflects all workflows enabled+disabled) |
| `version`        | string  | ACS version string                                                                        |
| `data_dir`       | string  | Filesystem path to the data directory                                                     |
| `service`        | object  | Platform service registration block (see below)                                           |

#### `service` block

| Field        | Type    | Description                                                                                              |
|--------------|---------|----------------------------------------------------------------------------------------------------------|
| `registered` | bool    | `true` when ACS is registered with the platform service manager                                          |
| `platform`   | string  | One of `"windows"`, `"macos"`, or `"linux"`                                                              |
| `details`    | string  | Filesystem path or registry location of the service registration. Omitted when `registered` is `false`.  |

The platform service manager is the Windows Run key (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`), the macOS launchd plist at `~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist`, or the Linux systemd user unit at `~/.config/systemd/user/agentcronsystem.service`.

---

### GET /api/workflows

List all workflows.

**Request:** No body, no query parameters.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns a JSON array of Workflow objects |
| 500 Internal Server Error | Storage failure |

```json
[
  {
    "id": "01941234-5678-7abc-def0-123456789abc",
    "name": "nightly-analysis",
    "version": 1,
    "schedule": "0 2 * * *",
    "timezone": "America/New_York",
    "schedule_mode": "Cron",
    "enabled": true,
    "steps": [
      {
        "kind": "shell",
        "id": "fetch",
        "command": "git pull origin main",
        "pass_stdin": false,
        "on_failure": null,
        "always_run": false,
        "timeout_secs": 60,
        "working_dir": null,
        "env_vars": null,
        "capture": { "stdout_max_bytes": 65536, "parser": null }
      }
    ],
    "default_input": null,
    "working_dir": "/workspace",
    "env_vars": { "LANG": "en_US.UTF-8" },
    "allow_concurrent": true,
    "on_failure": "abort",
    "last_run_at": "2025-01-16T02:00:00Z",
    "last_run_status": "Completed",
    "last_run_id": "01941234-aaaa-7abc-def0-123456789abc",
    "next_run_at": "2025-01-17T02:00:00Z",
    "created_at": "2025-01-15T10:30:00Z",
    "updated_at": "2025-01-15T10:30:00Z"
  }
]
```

The `next_run_at` field is computed at runtime for enabled workflows and is `null` for disabled workflows. It is never persisted to disk; the serde annotation `skip_deserializing` ensures it is always re-derived on read.

---

### POST /api/workflows

Create a new workflow.

**Request Body:** [NewWorkflow](#newworkflow) JSON object.

```json
{
  "name": "nightly-analysis",
  "schedule": "0 2 * * *",
  "timezone": "America/New_York",
  "schedule_mode": "Cron",
  "enabled": true,
  "steps": [
    {
      "kind": "shell",
      "id": "run",
      "command": "bash /opt/scripts/analyze.sh",
      "timeout_secs": 3600
    }
  ],
  "working_dir": "/workspace",
  "env_vars": { "LANG": "en_US.UTF-8" },
  "allow_concurrent": false,
  "on_failure": "abort"
}
```

**Response:**

| Status | Description |
|--------|-------------|
| 201 Created | Workflow created. Returns the full [Workflow](#workflow) object. |
| 400 Bad Request | `error: "command_template_removed"` — the payload contains an `AgentStep` with the removed `command_template` field. Migrate to `model` / `extra_args`. |
| 409 Conflict | A workflow with the same `name` already exists. |
| 422 Unprocessable Entity | Validation failed (empty name, UUID name, invalid cron, invalid timezone, no steps, duplicate step ids, invalid capture parser). |
| 500 Internal Server Error | Storage failure. |

**Side effects:** Broadcasts a `WorkflowChanged` SSE event with `change_kind: "created"` and notifies the scheduler.

---

### GET /api/workflows/{id}

Retrieve a single workflow by UUID or name.

**Path Parameters:**

| Parameter | Type   | Description                                  |
|-----------|--------|----------------------------------------------|
| `id`      | string | Workflow UUID or name (see [Workflow Identifier Resolution](#workflow-identifier-resolution)). |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the full [Workflow](#workflow) object. |
| 404 Not Found | No workflow matching the given UUID or name. |
| 500 Internal Server Error | Storage failure. |

---

### PATCH /api/workflows/{id}

Partially update an existing workflow. Only the fields you include in the request body will be changed. Changes to any of `name`, `schedule`, `timezone`, `schedule_mode`, `steps`, `default_input`, `working_dir`, `env_vars`, `allow_concurrent`, `on_failure` bump `version`. Toggling `enabled` alone does NOT bump `version`.

**Path Parameters:**

| Parameter | Type   | Description             |
|-----------|--------|-------------------------|
| `id`      | string | Workflow UUID or name.  |

**Request Body:** [WorkflowUpdate](#workflowupdate) JSON object. All fields are optional.

```json
{
  "schedule": "30 3 * * *",
  "enabled": false,
  "on_failure": "continue"
}
```

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the full updated [Workflow](#workflow) object. |
| 400 Bad Request | `error: "command_template_removed"` — the payload contains an `AgentStep` with the removed `command_template` field. Migrate to `model` / `extra_args`. |
| 404 Not Found | Workflow not found. |
| 409 Conflict | Another workflow already has the requested `name`. |
| 422 Unprocessable Entity | Validation failed on one or more fields. |
| 500 Internal Server Error | Storage failure. |

**Side effects:** Broadcasts a `WorkflowChanged` SSE event with `change_kind: "updated"` and notifies the scheduler.

---

### DELETE /api/workflows/{id}

Delete a workflow.

**Path Parameters:**

| Parameter | Type   | Description             |
|-----------|--------|-------------------------|
| `id`      | string | Workflow UUID or name.  |

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 204 No Content | Workflow deleted. No response body. |
| 404 Not Found | Workflow not found. |
| 500 Internal Server Error | Storage failure. |

**Side effects:** Broadcasts a `WorkflowChanged` SSE event with `change_kind: "deleted"`.

---

### POST /api/workflows/{id}/trigger

Manually trigger an immediate execution of the workflow, regardless of its cron schedule. Optionally accepts per-invocation parameters.

**Path Parameters:**

| Parameter | Type   | Description             |
|-----------|--------|-------------------------|
| `id`      | string | Workflow UUID or name.  |

**Request Body:** [TriggerParams](#triggerparams) JSON object. The body is optional; send `{}` to use all defaults. The `input` field defaults to `null` when omitted, falling back to the workflow's `default_input`.

```json
{
  "input": { "repo": "myorg/myrepo", "branch": "main" },
  "env": { "DEBUG": "1" },
  "target_step": null
}
```

| Field         | Type                      | Required | Default        | Description                                                                                                  |
|---------------|---------------------------|----------|----------------|--------------------------------------------------------------------------------------------------------------|
| `input`       | any JSON value            | No       | `null`         | Replaces `workflow.default_input` for this run. All step templates referencing `${input.*}` receive these values. When omitted or `null`, falls back to the workflow's `default_input`. Sending `{}` stores an empty object (not the default). |
| `env`         | object (string -> string) | No       | `null`         | Overlays onto `workflow.env_vars` for this run (merge, not replace). Trigger `env` wins on collision.       |
| `target_step` | string                    | No       | `null`         | Step `id` to route the trigger's `input` to as stdin. When set, the `input` value is serialized (strings as raw bytes, all other JSON as compact JSON) and written to that step's stdin when it executes. Only `Shell` and `Script` steps consume stdin — other step kinds ignore this value. A non-matching `id` is also ignored silently. `target_step` overrides `pass_stdin` for the matching step. |

**Response:**

| Status | Description |
|--------|-------------|
| 202 Accepted | The workflow run has been dispatched. |
| 404 Not Found | Workflow not found. |
| 409 Conflict | Workflow has `allow_concurrent: false` and a run is already active. No new run is created. |
| 500 Internal Server Error | Failed to create run record or spawn the executor. |

**409 Conflict body:**

```json
{
  "error": "concurrent_run_active",
  "message": "Workflow already has a running run; concurrent runs are disabled.",
  "active_run_id": "01941234-bbbb-7abc-def0-123456789abc"
}
```

| Field           | Type          | Description                                                  |
|-----------------|---------------|--------------------------------------------------------------|
| `error`         | string        | Always `"concurrent_run_active"` for this response.          |
| `message`       | string        | Human-readable explanation.                                  |
| `active_run_id` | string (UUID) | The `run_id` of the active run that blocked the new trigger. |

```json
{
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "workflow_version": 1,
  "run_url": "/api/runs/01941234-bbbb-7abc-def0-123456789abc"
}
```

| Field              | Type          | Description                                                                 |
|--------------------|---------------|-----------------------------------------------------------------------------|
| `run_id`           | string (UUID) | Pre-generated run identifier. Use immediately with `GET /api/runs/{run_id}` or SSE filters. |
| `workflow_id`      | string (UUID) | The workflow that was triggered.                                             |
| `workflow_version` | integer       | The workflow version at trigger time (snapshotted into the run record).     |
| `run_url`          | string        | Convenience URL for the run: `/api/runs/{run_id}`.                          |

The run record is persisted to the `WorkflowRunStore` with `status: "Running"` **before** the background task begins, so `GET /api/runs/{run_id}` immediately after trigger always returns a result rather than 404.

**Example:**

```sh
curl -X POST http://127.0.0.1:8377/api/workflows/nightly-analysis/trigger \
  -H "Content-Type: application/json" \
  -d '{"input": {"branch": "feature/x"}, "env": {"DRY_RUN": "1"}}'
```

---

### GET /api/workflows/{id}/runs

List execution runs for a specific workflow, with pagination. Returns latest-first.

**Path Parameters:**

| Parameter | Type   | Description             |
|-----------|--------|-------------------------|
| `id`      | string | Workflow UUID or name.  |

**Query Parameters:**

| Parameter | Type    | Required | Default | Description                                          |
|-----------|---------|----------|---------|------------------------------------------------------|
| `limit`   | integer | No       | `20`    | Maximum number of runs to return. Capped at `100`.   |
| `offset`  | integer | No       | `0`     | Number of runs to skip (for pagination).             |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns a paginated list of runs. |
| 404 Not Found | Workflow not found. |
| 500 Internal Server Error | Storage failure. |

```json
{
  "runs": [
    {
      "run_id": "01941234-bbbb-7abc-def0-123456789abc",
      "workflow_id": "01941234-5678-7abc-def0-123456789abc",
      "workflow_version": 1,
      "workflow_snapshot": { "...": "full Workflow object" },
      "started_at": "2025-01-16T02:00:00Z",
      "finished_at": "2025-01-16T02:05:30Z",
      "status": "Completed",
      "trigger_input": { "branch": "main" },
      "steps": [
        {
          "step_index": 0,
          "step_id": "run",
          "kind": "shell",
          "status": "Completed",
          "started_at": "2025-01-16T02:00:01Z",
          "finished_at": "2025-01-16T02:05:30Z",
          "exit_code": 0,
          "log_byte_offset_start": 0,
          "log_byte_offset_end": 4096,
          "cost_usd": null,
          "error": null
        }
      ],
      "total_cost_usd": null,
      "total_duration_ms": 330000
    }
  ],
  "total": 42
}
```

| Field   | Type    | Description                                              |
|---------|---------|----------------------------------------------------------|
| `runs`  | array   | Array of [WorkflowRun](#workflowrun) objects.           |
| `total` | integer | Total number of runs for this workflow (before pagination). |

---

### GET /api/runs/{run_id}

Retrieve a single run record with full step-level detail.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `run_id`  | string | The run UUID. Must be a valid UUID (name lookup is not supported for runs). |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the full [WorkflowRun](#workflowrun) object. |
| 400 Bad Request | `run_id` is not a valid UUID. |
| 404 Not Found | No run found for the given UUID. |
| 500 Internal Server Error | Storage failure. |

The response body is a [WorkflowRun](#workflowrun) object including the full `workflow_snapshot` (the complete workflow definition as it was at trigger time).

---

### POST /api/runs/{run_id}/kill

Request cancellation of a running workflow run. Sends a kill signal to the currently-executing step and updates the persisted run record to `status: "Killed"`.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `run_id`  | string | The run UUID. Must be a valid UUID.    |

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 202 Accepted | Kill signal sent (or best-effort if run already finished). |
| 400 Bad Request | `run_id` is not a valid UUID. |
| 404 Not Found | No run found for the given UUID. |
| 500 Internal Server Error | Failed to update run record. |

```json
{"message": "Kill signal sent"}
```

**Behavior:**

1. Looks up the run in the persistent store; returns 404 if not found.
2. Sends `true` on the per-run kill channel, causing the executor's `select!` loop to call `kill_process_tree` on the running step's PID. `HttpStep` cancels its in-flight `reqwest` request by dropping the future.
3. If the run is still `Running`, updates the persisted record to `status: "Killed"` and sets `finished_at` to now.

**Race note:** If a run finishes between the kill lookup and the status update, the handler may overwrite the executor's final `Completed` or `Failed` status with `Killed`. This is documented and accepted behavior.

---

### GET /api/runs/{run_id}/log

Fetch the on-disk run log as `text/plain`. The log holds the concatenated stdout/stderr of every step in execution order; each `StepRun` references its slice via `log_byte_offset_start` / `log_byte_offset_end`.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `run_id`  | string | The run UUID.                          |

**Query Parameters:**

| Parameter    | Type    | Required | Description                                                                                |
|--------------|---------|----------|--------------------------------------------------------------------------------------------|
| `step_index` | integer | No       | If supplied, return only the bytes belonging to the StepRun with this `step_index`.        |

When `step_index` is omitted the entire log file is returned.
When the requested step's `log_byte_offset_end` is `null` the response tails to end-of-file. `_end` is `null` only for the currently-running step or for steps that errored before their `write_step_start`/`write_step_end` markers landed (e.g. template-substitution or spawn failures); for Killed, Failed, and Timeout outcomes where the END marker was written, `_end` is populated and the slice is exact.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | `text/plain` body containing the requested bytes. |
| 400 Bad Request | `run_id` is not a valid UUID. |
| 404 Not Found | Run, requested `step_index`, or log file is missing. |
| 422 Unprocessable Entity | `error: "log_offset_out_of_range"` — the recorded `log_byte_offset_start` for the requested step extends past the actual log file length (e.g. the log was truncated or replaced after the run was persisted). |
| 500 Internal Server Error | Failed to read the log file. |

The log file lives at `<data_dir>/logs/<workflow_id>/<run_id>.log` and uses the boundary markers documented under [`StepRun`](#steprun).

---

### GET /api/events/workflows

Server-Sent Events (SSE) stream for real-time workflow execution and lifecycle events.

**Query Parameters:**

| Parameter     | Type   | Required | Default | Description                                                         |
|---------------|--------|----------|---------|---------------------------------------------------------------------|
| `run_id`      | string | No       | (none)  | Filter events to only those for this run UUID.                      |
| `workflow_id` | string | No       | (none)  | Filter events to only those for this workflow UUID.                 |

Both filter parameters must be valid UUIDs if provided. Invalid UUIDs are silently ignored (no filtering applied for that parameter).

**Important:** `WorkflowChanged` events carry a `workflow_id` but no `run_id`. When a `run_id` filter is active, `WorkflowChanged` events are **filtered out** because they do not carry a `run_id`. To receive both run-level events and workflow lifecycle events together, use only the `workflow_id` filter.

**Response:** An SSE stream (`text/event-stream`). The connection is kept alive with a keepalive comment every 15 seconds (text: `"keepalive"`).

Each SSE message has:
- `event:` -- the event type name (snake_case, see [SSE Event Types](#sse-event-types))
- `data:` -- a JSON-serialized `WorkflowEvent` object

**Connection behavior:**
- The stream stays open indefinitely until the client disconnects.
- If the client falls behind (broadcast channel lag), a comment `lagged: some workflow events were missed` is sent.

**Example SSE stream:**

```
event: run_started
data: {"type":"RunStarted","run_id":"...","workflow_id":"...","workflow_version":1,"started_at":"2025-01-16T02:00:00Z"}

event: step_started
data: {"type":"StepStarted","run_id":"...","workflow_id":"...","step_index":0,"step_id":"run","kind":"shell","started_at":"2025-01-16T02:00:01Z"}

event: step_output
data: {"type":"StepOutput","run_id":"...","workflow_id":"...","step_index":0,"step_id":"run","data":"Analyzing...\n","timestamp":"2025-01-16T02:00:02Z"}

event: step_completed
data: {"type":"StepCompleted","run_id":"...","workflow_id":"...","step_index":0,"step_id":"run","exit_code":0,"cost_usd":null,"finished_at":"2025-01-16T02:05:30Z"}

event: run_completed
data: {"type":"RunCompleted","run_id":"...","workflow_id":"...","status":"Completed","total_cost_usd":null,"finished_at":"2025-01-16T02:05:30Z"}

event: workflow_changed
data: {"type":"WorkflowChanged","workflow_id":"...","version":2,"change_kind":"updated"}

```

---

### POST /api/shutdown

Initiate a graceful shutdown of the daemon.

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Shutdown signal sent. |

```json
{
  "message": "Shutdown initiated"
}
```

The server will finish in-flight requests, then terminate. The response is sent before the actual shutdown occurs.

---

### POST /api/restart

Restart the daemon by spawning a new process and then shutting down the current one.

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Restart initiated. A new daemon process has been spawned. |
| 500 Internal Server Error | Failed to determine the executable path or spawn the new process. |

```json
{
  "message": "Restart initiated"
}
```

The current process shuts down after a 500ms delay (to allow the response to be delivered). The new process is started with the `start --foreground` arguments.

---

### GET /api/logs

Read the daemon's own log file (`daemon.log`).

**Query Parameters:**

| Parameter | Type    | Required | Default | Description                              |
|-----------|---------|----------|---------|------------------------------------------|
| `tail`    | integer | No       | (none)  | Return only the last N lines of the log. |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the daemon log content as `text/plain`. |
| 500 Internal Server Error | Failed to read the log file. |

The response `Content-Type` is `text/plain`.

If no daemon log file exists yet, the response body is:

```
No daemon logs available yet.
```

---

## Data Models

### Workflow

The full workflow object returned by GET, POST (201), and PATCH (200) endpoints.

| Field             | Type                                  | Nullable | Description                                                                        |
|-------------------|---------------------------------------|----------|------------------------------------------------------------------------------------|
| `id`              | string (UUID)                         | No       | Unique identifier, auto-generated as UUIDv7.                                       |
| `name`            | string                                | No       | Unique human-readable name (slug).                                                 |
| `version`         | integer (u32)                         | No       | Auto-incrementing version. Bumps on changes to any of `name`, `schedule`, `timezone`, `schedule_mode`, `steps`, `default_input`, `working_dir`, `env_vars`, `allow_concurrent`, or `on_failure`. Toggling `enabled` alone does NOT bump version. |
| `schedule`        | string                                | No       | Cron expression (5-field standard syntax).                                         |
| `timezone`        | string                                | Yes      | IANA timezone name, or `null` for UTC.                                             |
| `schedule_mode`   | string                                | No       | One of `"Cron"` or `"WaitForCompletion"`. Default: `"Cron"`.                      |
| `enabled`         | bool                                  | No       | Whether the workflow is scheduled.                                                 |
| `steps`           | array of [StepDef](#stepdef)          | No       | Ordered list of step definitions. Must contain at least one step.                  |
| `default_input`   | any JSON value                        | Yes      | Baseline trigger payload used for cron-fired runs (or manual triggers with no body). |
| `working_dir`     | string                                | Yes      | Default working directory for all steps (overridable per step).                    |
| `env_vars`        | object (string -> string)             | Yes      | Default environment variables for all steps (merged with per-step `env_vars`).    |
| `allow_concurrent`| bool                                  | No       | Whether multiple simultaneous runs of this workflow are permitted. Default: `true`.|
| `on_failure`      | [FailurePolicy](#failurepolicy)       | No       | Default failure policy for steps that do not specify their own. Default: `"abort"`.|
| `last_run_at`     | string (ISO 8601)                     | Yes      | When the workflow last ran, or `null` if never.                                    |
| `last_run_status` | [RunStatus](#runstatus)               | Yes      | Status of the last run, or `null` if never.                                        |
| `last_run_id`     | string (UUID)                         | Yes      | UUID of the last run, or `null` if never.                                          |
| `next_run_at`     | string (ISO 8601)                     | Yes      | Computed next scheduled run time. `null` for disabled workflows. Never persisted; always computed at read time. |
| `created_at`      | string (ISO 8601)                     | No       | When the workflow was created.                                                     |
| `updated_at`      | string (ISO 8601)                     | No       | When the workflow was last modified.                                               |

---

### NewWorkflow

Request body for `POST /api/workflows`.

| Field            | Type                                  | Required | Default    | Description                                                         |
|------------------|---------------------------------------|----------|------------|---------------------------------------------------------------------|
| `name`           | string                                | Yes      |            | Unique name. See [Validation Rules](#validation-rules).             |
| `schedule`       | string                                | Yes      |            | Cron expression (5-field).                                          |
| `steps`          | array of [StepDef](#stepdef)          | Yes      |            | At least one step required.                                         |
| `timezone`       | string                                | No       | daemon `display_timezone` (default `America/Los_Angeles`) | IANA timezone name. |
| `schedule_mode`  | string                                | No       | `"Cron"`   | One of `"Cron"` or `"WaitForCompletion"`.                           |
| `enabled`        | bool                                  | No       | `true`     | Whether the workflow starts enabled.                                |
| `default_input`  | any JSON value                        | No       | `null`     | Default trigger payload.                                            |
| `working_dir`    | string                                | No       | derived    | Default working directory. When omitted, the daemon creates `<user-documents>/agent-cron-scheduler/<sanitized-name>/` and stores its path. |
| `env_vars`       | object (string -> string)             | No       | `null`     | Default environment variables.                                      |
| `allow_concurrent`| bool                                 | No       | `true`     | Allow concurrent runs. `null` treated as `true`.                    |
| `on_failure`     | [FailurePolicy](#failurepolicy)       | No       | `"abort"`  | Workflow-level default failure policy.                              |

---

### WorkflowUpdate

Request body for `PATCH /api/workflows/{id}`. All fields are optional; only included fields are updated.

| Field            | Type                                  | Description                                                              |
|------------------|---------------------------------------|--------------------------------------------------------------------------|
| `name`           | string                                | New name. Same validation as creation.                                   |
| `schedule`       | string                                | New cron expression.                                                     |
| `timezone`       | string                                | New IANA timezone.                                                       |
| `schedule_mode`  | string                                | New scheduling mode.                                                     |
| `enabled`        | bool                                  | Enable or disable the workflow.                                          |
| `steps`          | array of [StepDef](#stepdef)          | Replace the entire step list. Must contain at least one step if provided.|
| `default_input`  | any JSON value                        | New default trigger payload.                                             |
| `working_dir`    | string                                | New default working directory.                                           |
| `env_vars`       | object (string -> string)             | New default environment variables (full replacement).                    |
| `allow_concurrent`| bool                                 | New concurrent-run setting.                                              |
| `on_failure`     | [FailurePolicy](#failurepolicy)       | New workflow-level failure policy.                                       |

Changes to any of `name`, `schedule`, `timezone`, `schedule_mode`, `steps`, `default_input`, `working_dir`, `env_vars`, `allow_concurrent`, `on_failure` bump `version`. Toggling `enabled` alone does NOT bump `version`.

Sending `null` for `timezone`, `working_dir`, or `default_input` in a PATCH body is treated the same as omitting the field — the existing value is unchanged. Send a non-null value to update them.

---

### StepDef

A tagged union representing one step in a workflow. Serialized with a `"kind"` discriminator field.

All step variants include the [StepDefCommon](#stepdefcommon) fields flattened at the top level (there is no nested `"common"` key in the JSON).

#### kind: shell

Executes a command via the system shell (`/bin/sh -c` on Unix, `cmd.exe /C` on Windows).

```json
{
  "kind": "shell",
  "id": "fetch",
  "command": "git pull origin ${input.branch}",
  "pass_stdin": false,
  "timeout_secs": 60,
  "on_failure": null,
  "always_run": false,
  "working_dir": null,
  "env_vars": null,
  "capture": { "stdout_max_bytes": 65536, "parser": null }
}
```

| Extra Field  | Type   | Default  | Description                                                                 |
|--------------|--------|----------|-----------------------------------------------------------------------------|
| `command`    | string | required | Shell command to execute. Supports template substitution.                   |
| `pass_stdin` | bool   | `false`  | Pipe the previous step's stdout into this step's stdin.                     |

#### kind: script

Executes a script file with an optional interpreter.

```json
{
  "kind": "script",
  "id": "deploy",
  "path": "/opt/scripts/deploy.sh",
  "script_type": "shell",
  "args": "--env ${input.env}",
  "pass_stdin": false,
  "timeout_secs": 300
}
```

| Extra Field   | Type   | Default  | Description                                                                                      |
|---------------|--------|----------|--------------------------------------------------------------------------------------------------|
| `path`        | string | required | Path to the script file.                                                                         |
| `script_type` | string | `null`   | Interpreter: `"shell"`, `"batch"`, `"python"`, `"powershell"`. `null` = infer from extension.  |
| `args`        | string | `null`   | Arguments appended to the script invocation. Supports template substitution.                    |
| `pass_stdin`  | bool   | `false`  | Pipe the previous step's stdout into this step's stdin.                                          |

#### kind: http

Makes an HTTP request using `reqwest`.

```json
{
  "kind": "http",
  "id": "notify",
  "method": "POST",
  "url": "https://hooks.example.com/notify",
  "headers": { "Authorization": "Bearer ${input.token}" },
  "body": "{\"status\": \"${steps.run.exit_code}\"}",
  "expect_status": [200, 201],
  "timeout_secs": 30
}
```

| Extra Field     | Type                      | Default          | Description                                                              |
|-----------------|---------------------------|------------------|--------------------------------------------------------------------------|
| `method`        | string                    | required         | HTTP method: `"GET"`, `"POST"`, `"PUT"`, `"PATCH"`, `"DELETE"`.         |
| `url`           | string                    | required         | Request URL. Supports template substitution.                             |
| `headers`       | object (string -> string) | `{}`             | Request headers. Values support template substitution.                   |
| `body`          | string                    | `null`           | Request body. Supports template substitution.                            |
| `expect_status` | array of integer (u16)    | `[200..300)`     | HTTP status codes treated as success. Failure outside this list.         |

#### kind: match

Evaluates an expression and dispatches to one of several branches.

```json
{
  "kind": "match",
  "id": "route",
  "expr": "${steps.run.exit_code}",
  "cases": {
    "0": [{ "kind": "shell", "id": "on-success", "command": "echo success" }],
    "1": [{ "kind": "shell", "id": "on-failure", "command": "echo failure" }]
  },
  "default": [{ "kind": "shell", "id": "on-other", "command": "echo other" }]
}
```

| Extra Field | Type                                          | Default  | Description                                                    |
|-------------|-----------------------------------------------|----------|----------------------------------------------------------------|
| `expr`      | string                                        | required | Template that evaluates to a string used for exact-match.      |
| `cases`     | object (string -> array of [StepDef](#stepdef))| required | Named branches. The evaluated `expr` selects the branch.       |
| `default`   | array of [StepDef](#stepdef)                  | `null`   | Branch taken when `expr` matches no case.                      |

#### kind: set_var

Sets named exports in the step context. No subprocess.

```json
{
  "kind": "set_var",
  "id": "prepare",
  "exports": {
    "TARGET_DIR": "/deploy/${input.env}",
    "SESSION_ID": "${steps.agent.exports.session_id}"
  }
}
```

| Extra Field | Type                      | Default  | Description                                            |
|-------------|---------------------------|----------|--------------------------------------------------------|
| `exports`   | object (string -> string) | required | Named exports to set. Values support template substitution. |

#### kind: agent

First-class invocation of an AI agent (currently Claude Code CLI).

```json
{
  "kind": "agent",
  "id": "review",
  "agent_type": "claude_code_cli",
  "prompt": "Review the diff at ${steps.fetch.exports.diff_path} and summarize issues.",
  "model": null,
  "extra_args": [],
  "timeout_secs": 120
}
```

| Extra Field  | Type           | Default  | Description                                                                                                |
|--------------|----------------|----------|------------------------------------------------------------------------------------------------------------|
| `agent_type` | string         | required | Agent to invoke. Currently only `"claude_code_cli"`.                                                       |
| `prompt`     | string         | required | Prompt string. Supports template substitution.                                                             |
| `model`      | string (nullable) | `null` | Optional Claude model identifier passed as `--model <value>`. Example: `claude-haiku-4-5-20251001`.       |
| `extra_args` | array\<string\> | `[]`    | Additional verbatim argv elements appended to the `claude` invocation.                                     |

For `claude_code_cli`, the runner builds argv directly — there is no template string to override. The canonical baseline argv is:
```
claude -p <resolved_prompt> --output-format stream-json --verbose --dangerously-skip-permissions
```
If `model` is set, `--model <value>` is inserted. If `extra_args` is non-empty, each item is appended verbatim as a separate argv element. The process is spawned directly (no shell wrapper), so shell-escaping concerns do not apply.

Cost is captured per `AgentStep` via streaming NDJSON parsing and stored in `StepRun.cost_usd` / `WorkflowRun.total_cost_usd`.

---

### StepDefCommon

Fields present (flattened) on every step variant.

| Field          | Type                                      | Default  | Description                                                                           |
|----------------|-------------------------------------------|----------|---------------------------------------------------------------------------------------|
| `id`           | string                                    | required | Stable step handle (e.g., `"fetch"`, `"review"`). Must be unique across the workflow. |
| `on_failure`   | [FailurePolicy](#failurepolicy) or `null` | `null`   | `null` = inherit `workflow.on_failure`.                                               |
| `always_run`   | bool                                      | `false`  | Run this step even if a prior step aborted the run.                                   |
| `timeout_secs` | integer (u64) or `null`                   | `null`   | Per-step timeout. `null` or `0` = no timeout.                                         |
| `working_dir`  | string or `null`                          | `null`   | Overrides `workflow.working_dir` for this step.                                       |
| `env_vars`     | object (string -> string) or `null`       | `null`   | Merged with `workflow.env_vars`; step values win on collision.                        |
| `capture`      | object                                    | see below| Capture configuration.                                                                |

**`capture` object:**

| Field              | Type    | Default  | Description                                                     |
|--------------------|---------|----------|-----------------------------------------------------------------|
| `stdout_max_bytes` | integer | `65536`  | Maximum bytes of stdout to capture (64 KB default).             |
| `parser`           | string  | `null`   | Output parser: `"json"`, `"lines"`, or `"raw"`. `null` = raw.  |

---

### FailurePolicy

Controls what happens when a step fails. Serialized as a string or tagged object.

| Variant  | JSON representation                                     | Description                                                  |
|----------|---------------------------------------------------------|--------------------------------------------------------------|
| `abort`  | `"abort"`                                               | (default) Stop the run immediately.                          |
| `continue`| `"continue"`                                           | Record failure in context and continue to the next step.     |
| `retry`  | `{"retry": {"attempts": 3, "backoff_ms": 1000}}`        | Retry the step up to `attempts` times with `backoff_ms` delay.|

---

### TriggerParams

Request body for `POST /api/workflows/{id}/trigger`.

| Field         | Type                      | Required | Description                                                                                |
|---------------|---------------------------|----------|--------------------------------------------------------------------------------------------|
| `input`       | any JSON value            | No       | Trigger payload; optional, defaults to `null`. When `null` or omitted, falls back to `workflow.default_input`. Sending `{}` stores an empty object rather than falling back to `default_input`. |
| `env`         | object (string -> string) | No       | Per-trigger environment variables. Merged onto `workflow.env_vars` (trigger wins on collision). |
| `target_step` | string                    | No       | Step `id` to route the trigger's `input` to as stdin. Strings are written as raw bytes; all other JSON values are serialized as compact JSON. Only `Shell` and `Script` steps consume stdin — other step kinds ignore this value. A non-matching `id` is also ignored silently. Overrides `pass_stdin` for the matching step. |

**Template substitution:** Fields marked as "template" in step definitions support two namespaces wrapped in `${...}` syntax:

- `input.<path>` — dotted path into the trigger payload (e.g., `${input.repo}`, `${input.user.name}`).
- `steps.<step_id>.<accessor>` — output from a prior step. Accessors: `stdout`, `exit_code`, `exports.<name>` (e.g., `${steps.fetch.stdout}`, `${steps.set_session.exports.session_id}`).

Missing references substitute to empty string with a warning logged.

---

### WorkflowRun

Represents a single execution of a workflow.

| Field               | Type                           | Nullable | Description                                                                              |
|---------------------|--------------------------------|----------|------------------------------------------------------------------------------------------|
| `run_id`            | string (UUID)                  | No       | Unique run identifier (UUIDv7).                                                          |
| `workflow_id`       | string (UUID)                  | No       | The workflow that was executed.                                                           |
| `workflow_version`  | integer (u32)                  | No       | The workflow version at trigger time.                                                    |
| `workflow_snapshot` | [Workflow](#workflow)          | No       | Full workflow definition snapshot taken at trigger time. Runs are self-contained.        |
| `started_at`        | string (ISO 8601)              | No       | When the run started.                                                                    |
| `finished_at`       | string (ISO 8601)              | Yes      | When the run finished, or `null` if still running.                                       |
| `status`            | [RunStatus](#runstatus)        | No       | Current run status.                                                                      |
| `trigger_input`     | any JSON value                 | Yes      | The trigger payload used for this run. `null` only when both `trigger.input` and `workflow.default_input` are absent (effective input is `Value::Null`); an explicit `{}` is stored as `{}`. |
| `steps`             | array of [StepRun](#steprun)   | No       | Step execution records in runtime order (flattened; branch steps appear inline).        |
| `total_cost_usd`    | number (f64)                   | Yes      | Summed cost across all `AgentStep` runs in USD. `null` if no agent steps ran.            |
| `total_duration_ms` | integer (u64)                  | Yes      | Total wall-clock duration in milliseconds. `null` while running.                         |

---

### StepRun

Represents the execution record for one step within a run.

| Field                  | Type                    | Nullable | Description                                                                  |
|------------------------|-------------------------|----------|------------------------------------------------------------------------------|
| `step_index`           | integer (usize)         | No       | Position in the runtime execution timeline (0-based).                        |
| `step_id`              | string                  | No       | Matches `StepDefCommon.id` from the workflow definition.                     |
| `kind`                 | string                  | No       | Step kind: `"shell"`, `"script"`, `"http"`, `"match"`, `"set_var"`, `"agent"`. |
| `status`               | [RunStatus](#runstatus) | No       | Execution status of this step.                                               |
| `started_at`           | string (ISO 8601)       | No       | When the step started.                                                       |
| `finished_at`          | string (ISO 8601)       | Yes      | When the step finished, or `null` if still running.                          |
| `exit_code`            | integer (i32)           | Yes      | Process exit code, or `null` for non-process steps (`set_var`, `match`).     |
| `log_byte_offset_start`| integer (u64)           | No       | Byte offset into the combined run log file where this step's output begins.  |
| `log_byte_offset_end`  | integer (u64)           | Yes      | Byte offset where this step's output ends. `null` while the step is running and also for steps that errored before their END marker landed (e.g. template-substitution or spawn failures); populated for Completed, Killed, Failed, and Timeout outcomes that reached `write_step_end`. |
| `cost_usd`             | number (f64)            | Yes      | Cost for this step in USD. Non-null only for `AgentStep`.                    |
| `error`                | string                  | Yes      | Human-readable error description on failure, or `null`.                      |

The captured stdout/stderr is not stored on the `StepRun`. Each record's
`log_byte_offset_start` / `log_byte_offset_end` pair frames its bytes inside
the combined run log file; fetch them via [`GET /api/runs/{run_id}/log`](#get-apirunsrun_idlog).

**Combined run log file location:**
```
<data_dir>/logs/<workflow_id>/<run_id>.log
```

Log output is append-only with step boundary markers:
```
===== ACS-<VERSION>:STEP:<step_id>:START:<iso8601> =====
<step stdout/stderr interleaved>
===== ACS-<VERSION>:STEP:<step_id>:END:exit=<code>:<iso8601> =====
```

---

### RunStatus

A string enum representing the state of a run or step.

| Value       | JSON string     | Description                                                                                    |
|-------------|-----------------|-----------------------------------------------------------------------------------------------|
| `Running`   | `"Running"`     | Execution is in progress.                                                                     |
| `Completed` | `"Completed"`   | Reached the end of all steps. Steps with `on_failure: continue` may have failed without aborting the run. |
| `Failed`    | `"Failed"`      | Run terminated early due to a step abort, timeout, or infrastructure error.                   |
| `Killed`    | `"Killed"`      | Externally terminated via `POST /api/runs/{run_id}/kill`, daemon shutdown, or concurrency policy. |

---

## SSE Event Types

The SSE stream at `GET /api/events/workflows` emits the following event types. Each event is a JSON object with a `"type"` field (used as the internal discriminator) and the event-specific fields at the same level.

The SSE `event:` line uses the snake_case name; the JSON `"type"` field uses PascalCase.

### run_started

Emitted when a workflow run begins.

SSE event name: `run_started`

```json
{
  "type": "RunStarted",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "workflow_version": 1,
  "started_at": "2025-01-16T02:00:00Z"
}
```

| Field              | Type     | Description                   |
|--------------------|----------|-------------------------------|
| `run_id`           | UUID     | The new run identifier.       |
| `workflow_id`      | UUID     | The workflow being executed.  |
| `workflow_version` | integer  | Version at trigger time.      |
| `started_at`       | ISO 8601 | When the run started.         |

### step_started

Emitted when an individual step begins execution.

SSE event name: `step_started`

```json
{
  "type": "StepStarted",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "step_index": 0,
  "step_id": "fetch",
  "kind": "shell",
  "started_at": "2025-01-16T02:00:01Z"
}
```

| Field        | Type     | Description                                                       |
|--------------|----------|-------------------------------------------------------------------|
| `run_id`     | UUID     | The run containing this step.                                     |
| `workflow_id`| UUID     | The workflow.                                                     |
| `step_index` | integer  | Position in the execution timeline (0-based).                     |
| `step_id`    | string   | Step identifier (matches `StepDefCommon.id`).                     |
| `kind`       | string   | Step kind: `"shell"`, `"script"`, `"http"`, `"match"`, `"set_var"`, `"agent"`. |
| `started_at` | ISO 8601 | When the step started.                                            |

### step_output

Emitted on each stdout/stderr chunk from a running step. Streamed in real time via `EventEmittingLogSink`.

SSE event name: `step_output`

```json
{
  "type": "StepOutput",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "step_index": 0,
  "step_id": "fetch",
  "data": "From origin\nAlready up to date.\n",
  "timestamp": "2025-01-16T02:00:02Z"
}
```

| Field        | Type     | Description                                      |
|--------------|----------|--------------------------------------------------|
| `run_id`     | UUID     | The run producing output.                        |
| `workflow_id`| UUID     | The workflow.                                    |
| `step_index` | integer  | Step position in the execution timeline.         |
| `step_id`    | string   | Step identifier.                                 |
| `data`       | string   | Output chunk (may contain newlines).             |
| `timestamp`  | ISO 8601 | When this chunk was captured.                    |

### step_completed

Emitted when a step finishes (success, failure, or kill).

SSE event name: `step_completed`

```json
{
  "type": "StepCompleted",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "step_index": 0,
  "step_id": "fetch",
  "exit_code": 0,
  "cost_usd": null,
  "finished_at": "2025-01-16T02:00:03Z"
}
```

| Field        | Type     | Description                                                      |
|--------------|----------|------------------------------------------------------------------|
| `run_id`     | UUID     | The run.                                                         |
| `workflow_id`| UUID     | The workflow.                                                    |
| `step_index` | integer  | Step position in the execution timeline.                         |
| `step_id`    | string   | Step identifier.                                                 |
| `exit_code`  | integer or null | Process exit code. `null` for non-process steps.          |
| `cost_usd`   | number or null  | Cost attributed to this step (non-null for `AgentStep` only). |
| `finished_at`| ISO 8601 | When the step finished.                                          |

### run_completed

Emitted when a workflow run finishes successfully (terminal status `Completed`). Runs that end as `Failed` or `Killed` emit [`run_failed`](#run_failed) instead.

SSE event name: `run_completed`

```json
{
  "type": "RunCompleted",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "status": "Completed",
  "total_cost_usd": 0.0042,
  "finished_at": "2025-01-16T02:05:30Z"
}
```

| Field           | Type     | Description                                     |
|-----------------|----------|-------------------------------------------------|
| `run_id`        | UUID     | The run that finished.                          |
| `workflow_id`   | UUID     | The workflow.                                   |
| `status`        | string   | Final [RunStatus](#runstatus).                  |
| `total_cost_usd`| number or null | Summed agent-step cost. `null` if none.   |
| `finished_at`   | ISO 8601 | When the run finished.                          |

### run_failed

Emitted when a run finishes in any non-success terminal state — that is, when the run's final `RunStatus` is `Failed` (a step aborted, timed out, or hit an infrastructure error) or `Killed` (cancelled via `POST /api/runs/{run_id}/kill`, daemon shutdown, or concurrency policy). The `error` field carries the first per-step error message recorded on the run; the final status is whatever was persisted on the `WorkflowRun` and can be retrieved via `GET /api/runs/{run_id}`.

SSE event name: `run_failed`

```json
{
  "type": "RunFailed",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "error": "PTY spawn failed: No such file or directory",
  "finished_at": "2025-01-16T02:00:01Z"
}
```

| Field        | Type     | Description                            |
|--------------|----------|----------------------------------------|
| `run_id`     | UUID     | The run that failed.                   |
| `workflow_id`| UUID     | The workflow.                          |
| `error`      | string   | Human-readable error description.      |
| `finished_at`| ISO 8601 | When the failure was recorded.         |

### workflow_changed

Emitted when a workflow's configuration changes.

SSE event name: `workflow_changed`

```json
{
  "type": "WorkflowChanged",
  "workflow_id": "01941234-5678-7abc-def0-123456789abc",
  "version": 2,
  "change_kind": "updated"
}
```

| Field         | Type    | Description                                                       |
|---------------|---------|-------------------------------------------------------------------|
| `workflow_id` | UUID    | The workflow that changed.                                        |
| `version`     | integer | Workflow version after the change.                                |
| `change_kind` | string  | One of: `"created"`, `"updated"`, `"deleted"`, `"enabled"`, `"disabled"`. |

**`change_kind` values:**

| Value      | Triggered by                        |
|------------|-------------------------------------|
| `created`  | `POST /api/workflows`               |
| `updated`  | `PATCH /api/workflows/{id}`         |
| `deleted`  | `DELETE /api/workflows/{id}`        |
| `enabled`  | (scheduler/daemon internal)         |
| `disabled` | (scheduler/daemon internal)         |

---

## Validation Rules

The following validation rules are enforced on workflow creation (`POST /api/workflows`) and update (`PATCH /api/workflows/{id}`):

### Name

- Must not be empty or whitespace-only.
- Must not be a valid UUID (to prevent ambiguity with the UUID-or-name resolution).
- Must be unique across all workflows.

### Schedule (Cron Expression)

- Parsed and validated at submission time using 5-field standard cron syntax (`minute hour day-of-month month day-of-week`).
- Invalid cron expressions return `422 Unprocessable Entity` with `error: "validation_error"` and a message starting with `"Invalid cron expression ..."`. Other validation failures return `422` with the same error code.

### Timezone

- Must be a valid IANA timezone name (e.g., `"America/New_York"`, `"Europe/London"`, `"UTC"`).
- Invalid timezone strings return a `422` with a message containing `"timezone"`.

### Steps

- Must contain at least one step.
- All step `id` values must be unique across the entire workflow definition, including nested steps inside `MatchStep` branches.

### Capture Parser

- The `capture.parser` field, when present, must be one of `"json"`, `"lines"`, or `"raw"`. Any other value returns a `422`.
