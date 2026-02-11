# ACS REST API Reference

This document provides a comprehensive reference for every endpoint exposed by the Agent Cron Scheduler (ACS) HTTP server.

Base URL: `http://127.0.0.1:8377` (default port; see [Configuration](configuration.md) for how to change it)

All request and response bodies use JSON (`Content-Type: application/json`) unless otherwise noted.

---

## Table of Contents

- [Conventions](#conventions)
- [Error Response Format](#error-response-format)
- [Job Identifier Resolution](#job-identifier-resolution)
- [Endpoints](#endpoints)
  - [GET /health](#get-health)
  - [GET /api/jobs](#get-apijobs)
  - [POST /api/jobs](#post-apijobs)
  - [GET /api/jobs/{id}](#get-apijobsid)
  - [PATCH /api/jobs/{id}](#patch-apijobsid)
  - [DELETE /api/jobs/{id}](#delete-apijobsid)
  - [POST /api/jobs/{id}/enable](#post-apijobsidenable)
  - [POST /api/jobs/{id}/disable](#post-apijobsiddisable)
  - [POST /api/jobs/{id}/trigger](#post-apijobsidtrigger)
  - [GET /api/jobs/{id}/runs](#get-apijobsidruns)
  - [GET /api/runs/{run_id}/log](#get-apirunsrun_idlog)
  - [GET /api/events](#get-apievents)
  - [POST /api/shutdown](#post-apishutdown)
  - [POST /api/restart](#post-apirestart)
  - [GET /api/logs](#get-apilogs)
  - [GET /api/service/status](#get-apiservicestatus)
- [Data Models](#data-models)
  - [Job](#job)
  - [NewJob](#newjob)
  - [JobUpdate](#jobupdate)
  - [ExecutionType](#executiontype)
  - [TriggerParams](#triggerparams)
  - [JobRun](#jobrun)
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

| `error` value       | Typical HTTP Status | Description                                      |
|----------------------|---------------------|--------------------------------------------------|
| `not_found`          | 404                 | The requested resource does not exist             |
| `validation_error`   | 400                 | Request body or parameters failed validation      |
| `conflict`           | 409                 | A resource with the same unique key already exists |
| `internal_error`     | 500                 | An unexpected server-side error occurred           |

---

## Job Identifier Resolution

All endpoints that accept an `{id}` path parameter support two lookup strategies:

1. **UUID** -- If the value parses as a valid UUID, the job is looked up by its `id` field.
2. **Name** -- If UUID parsing fails, the value is treated as a job name and looked up via `find_by_name`.

This means you can use either `GET /api/jobs/01941234-5678-7abc-def0-123456789abc` or `GET /api/jobs/my-backup-job` interchangeably.

If neither lookup finds a matching job, a `404 not_found` error is returned.

---

## Endpoints

### GET /health

Returns the daemon health status including uptime and job counts.

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
  "version": "0.1.0",
  "data_dir": "/home/user/.local/share/agent-cron-scheduler"
}
```

| Field            | Type    | Description                                   |
|------------------|---------|-----------------------------------------------|
| `status`         | string  | Always `"ok"` when the server is responsive   |
| `uptime_seconds` | integer | Seconds since the daemon process started       |
| `active_jobs`    | integer | Number of enabled jobs                         |
| `total_jobs`     | integer | Total number of jobs (enabled + disabled)      |
| `version`        | string  | ACS version string                             |
| `data_dir`       | string  | Filesystem path to the data directory. Returns `"unknown"` if not explicitly configured. |

---

### GET /api/jobs

List all jobs, optionally filtered by enabled status.

**Query Parameters:**

| Parameter | Type | Required | Default | Description                        |
|-----------|------|----------|---------|------------------------------------|
| `enabled` | bool | No       | (none)  | Filter by enabled state: `true` or `false`. Omit to return all jobs. |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns a JSON array of Job objects |
| 500 Internal Server Error | Storage failure |

```json
[
  {
    "id": "01941234-5678-7abc-def0-123456789abc",
    "name": "my-backup",
    "schedule": "0 2 * * *",
    "execution": {
      "type": "ShellCommand",
      "value": "backup.sh"
    },
    "enabled": true,
    "timezone": "America/New_York",
    "working_dir": "/home/user",
    "env_vars": { "BACKUP_DIR": "/mnt/backup" },
    "timeout_secs": 3600,
    "log_environment": false,
    "created_at": "2025-01-15T10:30:00Z",
    "updated_at": "2025-01-15T10:30:00Z",
    "last_run_at": "2025-01-16T02:00:00Z",
    "last_exit_code": 0,
    "next_run_at": "2025-01-17T02:00:00Z"
  }
]
```

The `next_run_at` field is computed at runtime for enabled jobs and is `null` for disabled jobs.

---

### POST /api/jobs

Create a new scheduled job.

**Request Body:** [NewJob](#newjob) JSON object.

```json
{
  "name": "my-backup",
  "schedule": "0 2 * * *",
  "execution": {
    "type": "ShellCommand",
    "value": "backup.sh"
  },
  "enabled": true,
  "timezone": "America/New_York",
  "working_dir": "/home/user",
  "env_vars": { "BACKUP_DIR": "/mnt/backup" },
  "timeout_secs": 3600,
  "log_environment": false
}
```

| Field            | Type                            | Required | Default | Description                                          |
|------------------|---------------------------------|----------|---------|------------------------------------------------------|
| `name`           | string                          | Yes      |         | Unique human-readable name. Cannot be empty, whitespace-only, or a valid UUID. |
| `schedule`       | string                          | Yes      |         | Cron expression. Standard 5-field cron syntax. |
| `execution`      | [ExecutionType](#executiontype) | Yes      |         | What to execute when the job triggers.               |
| `enabled`        | bool                            | No       | `true`  | Whether the job is active for scheduling.            |
| `timezone`       | string                          | No       | `null`  | IANA timezone name (e.g., `"America/New_York"`, `"Europe/London"`, `"UTC"`). |
| `working_dir`    | string                          | No       | `null`  | Working directory for the command.                   |
| `env_vars`       | object (string -> string)       | No       | `null`  | Environment variables to set for the command.        |
| `timeout_secs`   | integer (u64)                   | No       | `0`     | Maximum execution time in seconds. `0` means no timeout. |
| `log_environment`| bool                            | No       | `false` | Whether to log environment variables in the run output. |

**Response:**

| Status | Description |
|--------|-------------|
| 201 Created | Job created successfully. Returns the full [Job](#job) object. |
| 400 Bad Request | Validation failed (empty name, UUID name, invalid cron, invalid timezone). |
| 409 Conflict | A job with the same `name` already exists. |
| 500 Internal Server Error | Storage failure. |

**Example success response (201):**

```json
{
  "id": "01941234-5678-7abc-def0-123456789abc",
  "name": "my-backup",
  "schedule": "0 2 * * *",
  "execution": {
    "type": "ShellCommand",
    "value": "backup.sh"
  },
  "enabled": true,
  "timezone": "America/New_York",
  "working_dir": "/home/user",
  "env_vars": { "BACKUP_DIR": "/mnt/backup" },
  "timeout_secs": 3600,
  "log_environment": false,
  "created_at": "2025-01-15T10:30:00Z",
  "updated_at": "2025-01-15T10:30:00Z",
  "last_run_at": null,
  "last_exit_code": null,
  "next_run_at": null
}
```

**Example error response (400):**

```json
{
  "error": "validation_error",
  "message": "Cron error: Invalid cron expression 'not a cron': ..."
}
```

**Example error response (409):**

```json
{
  "error": "conflict",
  "message": "A job with name 'my-backup' already exists"
}
```

**Side effects:** Broadcasts a `JobChanged` SSE event with `change: "Added"` and notifies the scheduler to pick up the new job.

---

### GET /api/jobs/{id}

Retrieve a single job by UUID or name.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name (see [Job Identifier Resolution](#job-identifier-resolution)). |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the full [Job](#job) object. |
| 404 Not Found | No job matching the given UUID or name. |
| 500 Internal Server Error | Storage failure. |

The `next_run_at` field is computed at runtime for enabled jobs.

```json
{
  "id": "01941234-5678-7abc-def0-123456789abc",
  "name": "my-backup",
  "schedule": "0 2 * * *",
  "execution": {
    "type": "ShellCommand",
    "value": "backup.sh"
  },
  "enabled": true,
  "timezone": "America/New_York",
  "working_dir": "/home/user",
  "env_vars": { "BACKUP_DIR": "/mnt/backup" },
  "timeout_secs": 3600,
  "log_environment": false,
  "created_at": "2025-01-15T10:30:00Z",
  "updated_at": "2025-01-15T10:30:00Z",
  "last_run_at": "2025-01-16T02:00:00Z",
  "last_exit_code": 0,
  "next_run_at": "2025-01-17T02:00:00Z"
}
```

---

### PATCH /api/jobs/{id}

Partially update an existing job. Only the fields you include in the request body will be changed.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name. |

**Request Body:** [JobUpdate](#jobupdate) JSON object. All fields are optional.

```json
{
  "name": "renamed-backup",
  "schedule": "30 3 * * *",
  "execution": {
    "type": "ScriptFile",
    "value": "/opt/scripts/backup.sh"
  },
  "enabled": false,
  "timezone": "Europe/London",
  "working_dir": "/opt",
  "env_vars": { "MODE": "full" },
  "timeout_secs": 7200,
  "log_environment": true
}
```

| Field            | Type                            | Required | Description                                |
|------------------|---------------------------------|----------|--------------------------------------------|
| `name`           | string                          | No       | New name. Same validation as creation.     |
| `schedule`       | string                          | No       | New cron expression.                       |
| `execution`      | [ExecutionType](#executiontype) | No       | New execution configuration.               |
| `enabled`        | bool                            | No       | Enable or disable the job.                 |
| `timezone`       | string                          | No       | New IANA timezone.                         |
| `working_dir`    | string                          | No       | New working directory.                     |
| `env_vars`       | object (string -> string)       | No       | New environment variables (replaces all).  |
| `timeout_secs`   | integer (u64)                   | No       | New timeout in seconds.                    |
| `log_environment`| bool                            | No       | New log_environment setting.               |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Job updated. Returns the full updated [Job](#job) object. |
| 400 Bad Request | Validation failed on one or more fields. |
| 404 Not Found | Job not found. |
| 409 Conflict | Another job already has the requested `name`. |
| 500 Internal Server Error | Storage failure. |

**Side effects:** Broadcasts a `JobChanged` SSE event with `change: "Updated"` and notifies the scheduler.

---

### DELETE /api/jobs/{id}

Delete a job and kill its active run (if any).

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name. |

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 204 No Content | Job deleted successfully. No response body. |
| 404 Not Found | Job not found. |
| 500 Internal Server Error | Storage failure. |

**Side effects:**
- If the job has an active run, it is killed via the kill channel.
- Broadcasts a `JobChanged` SSE event with `change: "Removed"`.
- Notifies the scheduler.

---

### POST /api/jobs/{id}/enable

Enable a previously disabled job.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name. |

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the full updated [Job](#job) object with `enabled: true`. |
| 404 Not Found | Job not found. |
| 500 Internal Server Error | Storage failure. |

**Side effects:** Broadcasts a `JobChanged` SSE event with `change: "Enabled"` and notifies the scheduler.

---

### POST /api/jobs/{id}/disable

Disable a job so it stops being scheduled.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name. |

**Request:** No body.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the full updated [Job](#job) object with `enabled: false`. |
| 404 Not Found | Job not found. |
| 500 Internal Server Error | Storage failure. |

**Side effects:** Broadcasts a `JobChanged` SSE event with `change: "Disabled"` and notifies the scheduler.

---

### POST /api/jobs/{id}/trigger

Manually trigger an immediate execution of the job, regardless of its cron schedule. Optionally accepts per-invocation parameters that override job defaults for a single run.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name. |

**Request Body:** Optional [TriggerParams](#triggerparams) JSON object. An empty body (or no `Content-Type` header) preserves backward compatibility and triggers the job with its default configuration.

```json
{
  "args": "--verbose --dry-run",
  "env": {
    "MODE": "manual",
    "DEBUG": "1"
  },
  "input": "data sent to stdin"
}
```

| Field   | Type                      | Required | Default | Description                                                              |
|---------|---------------------------|----------|---------|--------------------------------------------------------------------------|
| `args`  | string                    | No       | `null`  | Extra arguments appended to the job's command string for this run only.  |
| `env`   | object (string -> string) | No       | `null`  | Per-trigger environment variables. Override job-level `env_vars` for this run. |
| `input` | string                    | No       | `null`  | Data written to the process's stdin after spawn, then EOF.               |

**Response:**

| Status | Description |
|--------|-------------|
| 202 Accepted | The job has been dispatched for execution. |
| 400 Bad Request | Invalid JSON in request body. |
| 404 Not Found | Job not found. |
| 500 Internal Server Error | Failed to dispatch the job to the executor. |

```json
{
  "message": "Job triggered",
  "job_id": "01941234-5678-7abc-def0-123456789abc",
  "job_name": "my-backup",
  "run_id": "01941234-bbbb-7abc-def0-123456789abc"
}
```

| Field      | Type          | Description                                    |
|------------|---------------|------------------------------------------------|
| `message`  | string        | Always `"Job triggered"`.                      |
| `job_id`   | string (UUID) | The job that was triggered.                    |
| `job_name` | string        | Human-readable job name.                       |
| `run_id`   | string (UUID) | Pre-generated run identifier (UUIDv7). Can be used immediately to filter SSE events or poll for run status. |

The `run_id` is generated before the job is dispatched, so it is available in the response without waiting for execution to begin.

**Example request with trigger parameters:**

```sh
curl -X POST http://127.0.0.1:8377/api/jobs/my-backup/trigger \
  -H "Content-Type: application/json" \
  -d '{"args": "--full", "env": {"BACKUP_MODE": "full"}, "input": "yes"}'
```

**Example request without trigger parameters (backward compatible):**

```sh
curl -X POST http://127.0.0.1:8377/api/jobs/my-backup/trigger
```

**Example error response (400):**

```json
{
  "error": "validation_error",
  "message": "Invalid trigger body: expected value at line 1 column 1"
}
```

**Trigger parameter behavior:**

- **`args`**: Appended to the job's base command. For a `ShellCommand` with value `"backup.sh"` and trigger args `"--full"`, the effective command becomes `"backup.sh --full"`. The same concatenation applies to `ScriptFile` jobs.
- **`env`**: Merged with the job's `env_vars`. Trigger environment variables take the highest precedence: inherited system env < job `env_vars` < trigger `env`.
- **`input`**: Written to the spawned process's stdin immediately after launch, then stdin is closed (EOF). Useful for commands that read from stdin.

**Edge case:** If the daemon's internal dispatch channel is not available (e.g., the scheduler/executor subsystem has not fully initialized), the endpoint still returns `202 Accepted` but the job will not actually execute. This is a transient condition that can occur during daemon startup.

---

### GET /api/jobs/{id}/runs

List execution runs for a specific job, with pagination.

**Path Parameters:**

| Parameter | Type   | Description                            |
|-----------|--------|----------------------------------------|
| `id`      | string | Job UUID or job name. |

**Query Parameters:**

| Parameter | Type    | Required | Default | Description                                     |
|-----------|---------|----------|---------|-------------------------------------------------|
| `limit`   | integer | No       | `20`    | Maximum number of runs to return.               |
| `offset`  | integer | No       | `0`     | Number of runs to skip (for pagination).        |
| `status`  | string  | No       | (none)  | Filter by run status. Case-insensitive. Accepted values: `running`, `completed`, `failed`, `killed`. **Note:** The status filter is applied *after* pagination (`limit`/`offset`), so the returned list may contain fewer items than `limit` even if more matching runs exist. The `total` field reflects the pre-filter count. |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns a paginated list of runs. |
| 404 Not Found | Job not found. |
| 500 Internal Server Error | Storage failure. |

```json
{
  "runs": [
    {
      "run_id": "01941234-aaaa-7abc-def0-123456789abc",
      "job_id": "01941234-5678-7abc-def0-123456789abc",
      "started_at": "2025-01-16T02:00:00Z",
      "finished_at": "2025-01-16T02:05:30Z",
      "status": "Completed",
      "exit_code": 0,
      "log_size_bytes": 4096,
      "error": null
    }
  ],
  "total": 42,
  "limit": 20,
  "offset": 0
}
```

| Field    | Type    | Description                                   |
|----------|---------|-----------------------------------------------|
| `runs`   | array   | Array of [JobRun](#jobrun) objects.           |
| `total`  | integer | Total number of runs for this job (before pagination). |
| `limit`  | integer | The limit that was applied.                   |
| `offset` | integer | The offset that was applied.                  |

---

### GET /api/runs/{run_id}/log

Retrieve the output log for a specific run.

**Path Parameters:**

| Parameter | Type   | Description                                      |
|-----------|--------|--------------------------------------------------|
| `run_id`  | string | The run UUID. Must be a valid UUID (name lookup is not supported for runs). |

**Query Parameters:**

| Parameter | Type    | Required | Default | Description                                   |
|-----------|---------|----------|---------|-----------------------------------------------|
| `tail`    | integer | No       | (none)  | Return only the last N lines of the log.      |
| `format`  | string  | No       | (none)  | Accepted but ignored; reserved for forward compatibility. |

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns the log content as `text/plain`. |
| 400 Bad Request | Invalid `run_id` format (not a valid UUID). |
| 404 Not Found | No log found for the given run ID. |
| 500 Internal Server Error | Storage failure. |

The response body is plain text, not JSON. The `Content-Type` header is set to `text/plain`.

```
[2025-01-16T02:00:01Z] Starting backup...
[2025-01-16T02:03:15Z] Copied 1,234 files
[2025-01-16T02:05:30Z] Backup completed successfully
```

---

### GET /api/events

Server-Sent Events (SSE) stream for real-time job execution and lifecycle events.

**Query Parameters:**

| Parameter | Type   | Required | Default | Description                                           |
|-----------|--------|----------|---------|-------------------------------------------------------|
| `job_id`  | string | No       | (none)  | Filter events to only those for this job UUID.        |
| `run_id`  | string | No       | (none)  | Filter events to only those for this run UUID.        |

Both filter parameters must be valid UUIDs if provided. Invalid UUIDs are silently ignored (no filtering applied for that parameter).

**Important:** When a `run_id` filter is active, `JobChanged` events are **filtered out** because they do not carry a `run_id`. If you need both run-specific events and job lifecycle events, use only the `job_id` filter.

**Response:** An SSE stream (`text/event-stream`). The connection is kept alive with a keepalive comment every 15 seconds.

Each SSE message has:
- `event:` -- the event type name (see [SSE Event Types](#sse-event-types))
- `data:` -- a JSON-serialized `JobEvent` object

**Connection behavior:**
- The stream stays open indefinitely until the client disconnects.
- If the client falls behind (broadcast channel lag), a comment `lagged: some events were missed` is sent.
- Keepalive messages are sent as SSE comments (`: keepalive`) every 15 seconds.

**Example SSE stream:**

```
event: started
data: {"event":"Started","data":{"job_id":"01941234-5678-7abc-def0-123456789abc","run_id":"01941234-aaaa-7abc-def0-123456789abc","job_name":"my-backup","timestamp":"2025-01-16T02:00:00Z"}}

event: output
data: {"event":"Output","data":{"job_id":"01941234-5678-7abc-def0-123456789abc","run_id":"01941234-aaaa-7abc-def0-123456789abc","data":"Starting backup...\n","timestamp":"2025-01-16T02:00:01Z"}}

event: completed
data: {"event":"Completed","data":{"job_id":"01941234-5678-7abc-def0-123456789abc","run_id":"01941234-aaaa-7abc-def0-123456789abc","exit_code":0,"timestamp":"2025-01-16T02:05:30Z"}}

event: job_changed
data: {"event":"JobChanged","data":{"job_id":"01941234-5678-7abc-def0-123456789abc","change":"Updated","timestamp":"2025-01-16T03:00:00Z"}}

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
| `format`  | string  | No       | (none)  | Accepted but ignored; reserved for forward compatibility. |

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

### GET /api/service/status

Check the platform service installation and running status.

**Request:** No body, no query parameters.

**Response:**

| Status | Description |
|--------|-------------|
| 200 OK | Returns service status information. |

```json
{
  "platform": "windows",
  "service_installed": false,
  "service_running": false
}
```

| Field              | Type    | Description                                              |
|--------------------|---------|----------------------------------------------------------|
| `platform`         | string  | One of `"windows"`, `"macos"`, or `"linux"`.             |
| `service_installed`| bool    | Whether the system service is installed. Currently returns a static `false` value; actual service detection is not yet implemented. |
| `service_running`  | bool    | Whether the system service is currently running. Currently returns a static `false` value. |

---

## Data Models

### Job

The full job object returned by GET, POST, and PATCH endpoints.

| Field            | Type                            | Nullable | Description                                                  |
|------------------|---------------------------------|----------|--------------------------------------------------------------|
| `id`             | string (UUID)                   | No       | Unique identifier, auto-generated as UUIDv7.                |
| `name`           | string                          | No       | Unique human-readable name.                                  |
| `schedule`       | string                          | No       | Cron expression.                                             |
| `execution`      | [ExecutionType](#executiontype) | No       | What to execute.                                             |
| `enabled`        | bool                            | No       | Whether the job is scheduled.                                |
| `timezone`       | string                          | Yes      | IANA timezone name, or `null` for UTC.                       |
| `working_dir`    | string                          | Yes      | Working directory for the command, or `null`.                |
| `env_vars`       | object (string -> string)       | Yes      | Environment variables map, or `null`.                        |
| `timeout_secs`   | integer (u64)                   | No       | Max execution time in seconds. `0` = no timeout.            |
| `log_environment`| bool                            | No       | Whether to log environment variables in run output.          |
| `created_at`     | string (ISO 8601)               | No       | When the job was created.                                    |
| `updated_at`     | string (ISO 8601)               | No       | When the job was last modified.                              |
| `last_run_at`    | string (ISO 8601)               | Yes      | When the job last ran, or `null` if never.                   |
| `last_exit_code` | integer (i32)                   | Yes      | Exit code of the last run, or `null`.                        |
| `next_run_at`    | string (ISO 8601)               | Yes      | Computed next scheduled run time. `null` in POST and PATCH responses (computed at runtime only for GET endpoints). `null` for disabled jobs. |

### NewJob

Request body for `POST /api/jobs`.

| Field            | Type                            | Required | Default | Description                              |
|------------------|---------------------------------|----------|---------|------------------------------------------|
| `name`           | string                          | Yes      |         | Unique name. See [Validation Rules](#validation-rules). |
| `schedule`       | string                          | Yes      |         | Cron expression.                         |
| `execution`      | [ExecutionType](#executiontype) | Yes      |         | What to execute.                         |
| `enabled`        | bool                            | No       | `true`  | Whether the job starts enabled.          |
| `timezone`       | string                          | No       | `null`  | IANA timezone name.                      |
| `working_dir`    | string                          | No       | `null`  | Working directory.                       |
| `env_vars`       | object (string -> string)       | No       | `null`  | Environment variables.                   |
| `timeout_secs`   | integer (u64)                   | No       | `0`     | Timeout in seconds (`0` = no timeout).   |
| `log_environment`| bool                            | No       | `false` | Log environment variables.               |

### JobUpdate

Request body for `PATCH /api/jobs/{id}`. All fields are optional; only included fields are updated.

| Field            | Type                            | Description                              |
|------------------|---------------------------------|------------------------------------------|
| `name`           | string                          | New name. Same validation as creation.   |
| `schedule`       | string                          | New cron expression.                     |
| `execution`      | [ExecutionType](#executiontype) | New execution config.                    |
| `enabled`        | bool                            | New enabled state.                       |
| `timezone`       | string                          | New IANA timezone.                       |
| `working_dir`    | string                          | New working directory.                   |
| `env_vars`       | object (string -> string)       | New environment variables (full replace).|
| `timeout_secs`   | integer (u64)                   | New timeout in seconds.                  |
| `log_environment`| bool                            | New log_environment flag.                |

Note: The `last_run_at` and `last_exit_code` fields cannot be set via the API. They are updated internally by the executor.

### ExecutionType

A tagged union representing what the job executes. Serialized with `"type"` and `"value"` fields.

**Variant: ShellCommand**

Executes a shell command via the system shell.

```json
{
  "type": "ShellCommand",
  "value": "echo hello && date"
}
```

**Variant: ScriptFile**

Executes a script file.

```json
{
  "type": "ScriptFile",
  "value": "/opt/scripts/deploy.sh"
}
```

### TriggerParams

Optional request body for `POST /api/jobs/{id}/trigger`. All fields are optional. When the entire body is omitted or empty, the job runs with its default configuration.

| Field   | Type                      | Required | Default | Description                                                              |
|---------|---------------------------|----------|---------|--------------------------------------------------------------------------|
| `args`  | string                    | No       | `null`  | Extra arguments appended to the job's command string. For a `ShellCommand` with value `"cmd"`, the effective command becomes `"cmd <args>"`. Same for `ScriptFile`. |
| `env`   | object (string -> string) | No       | `null`  | Per-trigger environment variables. These override the job's `env_vars` for this single run (highest precedence: inherited env < job `env_vars` < trigger `env`). |
| `input` | string                    | No       | `null`  | Data written to the process's stdin immediately after spawn. Stdin is then closed (EOF). |

**Example:**

```json
{
  "args": "--verbose --dry-run",
  "env": {
    "MODE": "manual"
  },
  "input": "confirm"
}
```

### JobRun

Represents a single execution of a job.

| Field            | Type              | Nullable | Description                                    |
|------------------|-------------------|----------|------------------------------------------------|
| `run_id`         | string (UUID)     | No       | Unique run identifier (UUIDv7).               |
| `job_id`         | string (UUID)     | No       | The job that was executed.                     |
| `started_at`     | string (ISO 8601) | No       | When the run started.                          |
| `finished_at`    | string (ISO 8601) | Yes      | When the run finished, or `null` if still running. |
| `status`         | [RunStatus](#runstatus) | No  | Current run status.                            |
| `exit_code`      | integer (i32)     | Yes      | Process exit code, or `null` if not yet finished or if the process was killed. |
| `log_size_bytes` | integer (u64)     | No       | Size of the log output in bytes.               |
| `error`          | string            | Yes      | Error message if the run failed to start (e.g., PTY spawn failure), or `null`. |
| `trigger_params` | [TriggerParams](#triggerparams) | Yes | Trigger-time parameter overrides used for this run. Absent from the JSON response when `null` (omitted via `skip_serializing_if`). Only present when the run was triggered with per-invocation parameters. |

### RunStatus

A string enum representing the state of a job run.

| Value       | Description                                     |
|-------------|-------------------------------------------------|
| `Running`   | The job is currently executing.                 |
| `Completed` | The job finished with an exit code.             |
| `Failed`    | The job failed to start or encountered an error.|
| `Killed`    | The job was forcefully terminated (daemon shutdown, job deletion, or user-initiated kill). |

---

## SSE Event Types

The SSE stream at `GET /api/events` emits the following event types. Each event is serialized as a tagged JSON object with `"event"` and `"data"` fields at the top level.

### started

Emitted when a job run begins.

SSE event name: `started`

```json
{
  "event": "Started",
  "data": {
    "job_id": "01941234-5678-7abc-def0-123456789abc",
    "run_id": "01941234-aaaa-7abc-def0-123456789abc",
    "job_name": "my-backup",
    "timestamp": "2025-01-16T02:00:00Z"
  }
}
```

| Field      | Type   | Description                |
|------------|--------|----------------------------|
| `job_id`   | UUID   | The job being executed.    |
| `run_id`   | UUID   | The new run identifier.    |
| `job_name` | string | Human-readable job name.   |
| `timestamp`| ISO 8601 | When the run started.    |

### output

Emitted when a job produces stdout/stderr output.

SSE event name: `output`

```json
{
  "event": "Output",
  "data": {
    "job_id": "01941234-5678-7abc-def0-123456789abc",
    "run_id": "01941234-aaaa-7abc-def0-123456789abc",
    "data": "Copying files...\n",
    "timestamp": "2025-01-16T02:00:05Z"
  }
}
```

| Field      | Type   | Description                          |
|------------|--------|--------------------------------------|
| `job_id`   | UUID   | The job producing output.            |
| `run_id`   | UUID   | The run producing output.            |
| `data`     | string | The output text (may contain newlines). |
| `timestamp`| ISO 8601 | When this output was captured.     |

### completed

Emitted when a job run finishes successfully.

SSE event name: `completed`

```json
{
  "event": "Completed",
  "data": {
    "job_id": "01941234-5678-7abc-def0-123456789abc",
    "run_id": "01941234-aaaa-7abc-def0-123456789abc",
    "exit_code": 0,
    "timestamp": "2025-01-16T02:05:30Z"
  }
}
```

| Field       | Type    | Description                           |
|-------------|---------|---------------------------------------|
| `job_id`    | UUID    | The job that completed.               |
| `run_id`    | UUID    | The run that completed.               |
| `exit_code` | integer | Process exit code.                    |
| `timestamp` | ISO 8601 | When the run finished.              |

### failed

Emitted when a job run fails (e.g., process could not start).

SSE event name: `failed`

```json
{
  "event": "Failed",
  "data": {
    "job_id": "01941234-5678-7abc-def0-123456789abc",
    "run_id": "01941234-aaaa-7abc-def0-123456789abc",
    "error": "PTY spawn failed: No such file or directory",
    "timestamp": "2025-01-16T02:00:01Z"
  }
}
```

| Field      | Type   | Description                            |
|------------|--------|----------------------------------------|
| `job_id`   | UUID   | The job that failed.                   |
| `run_id`   | UUID   | The run that failed.                   |
| `error`    | string | Human-readable error description.      |
| `timestamp`| ISO 8601 | When the failure was recorded.       |

### job_changed

Emitted when a job's configuration or lifecycle state changes.

SSE event name: `job_changed`

```json
{
  "event": "JobChanged",
  "data": {
    "job_id": "01941234-5678-7abc-def0-123456789abc",
    "change": "Updated",
    "timestamp": "2025-01-16T03:00:00Z"
  }
}
```

| Field      | Type   | Description                            |
|------------|--------|----------------------------------------|
| `job_id`   | UUID   | The job that changed.                  |
| `change`   | string | One of: `"Added"`, `"Updated"`, `"Removed"`, `"Enabled"`, `"Disabled"`. |
| `timestamp`| ISO 8601 | When the change occurred.            |

**JobChangeKind values:**

| Value      | Triggered by                          |
|------------|---------------------------------------|
| `Added`    | `POST /api/jobs` (new job created)    |
| `Updated`  | `PATCH /api/jobs/{id}`                |
| `Removed`  | `DELETE /api/jobs/{id}`               |
| `Enabled`  | `POST /api/jobs/{id}/enable`          |
| `Disabled` | `POST /api/jobs/{id}/disable`         |

---

## Validation Rules

The following validation rules are enforced on job creation (`POST /api/jobs`) and update (`PATCH /api/jobs/{id}`):

### Name

- Must not be empty or whitespace-only.
- Must not be a valid UUID (to prevent ambiguity with the ID-or-name resolution).
- Must be unique across all jobs. Uniqueness is checked explicitly before creation or rename.

### Schedule (Cron Expression)

- Parsed and validated at submission time.
- Uses standard 5-field cron syntax (`minute hour day-of-month month day-of-week`). See [Job Management](job-management.md#cron-expressions) for full syntax details.
- Invalid expressions return a `400` with error code `validation_error` and a message starting with `"Cron error: ..."`.

### Timezone

- Must be a valid IANA timezone name (e.g., `"America/New_York"`, `"Europe/London"`, `"UTC"`).
- Invalid timezone strings return a `400` with a message containing `"Invalid timezone"`.

### Timeout

- The `timeout_secs` field is a `u64`. A value of `0` means no timeout.

### Execution

- Must be one of the two tagged variants: `ShellCommand` or `ScriptFile`.
- The `value` field is a string in both cases.
- An invalid or missing `type` field will cause a JSON deserialization error (400).
