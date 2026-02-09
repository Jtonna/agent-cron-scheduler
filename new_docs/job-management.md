# Job Management

This document describes how jobs are defined, validated, scheduled, and executed in the Agent Cron Scheduler (ACS) system.

---

## Job Model

A job represents a scheduled command or script that ACS executes on a cron-based schedule. The full `Job` struct contains the following fields:

| Field | Type | Description |
|---|---|---|
| `id` | `Uuid` (v7) | Unique identifier, auto-generated on creation. |
| `name` | `String` | Human-readable name. Must be unique across all jobs. Used to reference jobs in CLI commands and API calls. |
| `schedule` | `String` | Cron expression defining when the job runs. Validated against the `croner` crate. |
| `execution` | `ExecutionType` | What to execute -- either an inline shell command or a script file path. See [Execution Types](#execution-types). |
| `enabled` | `bool` | Whether the scheduler should run this job. Defaults to `true` on creation. |
| `timezone` | `Option<String>` | IANA timezone string for schedule evaluation. `None` means UTC. See [Timezone Support](#timezone-support). |
| `working_dir` | `Option<String>` | Optional working directory override for the spawned process. |
| `env_vars` | `Option<HashMap<String, String>>` | Optional per-job environment variables injected into the process. |
| `timeout_secs` | `u64` | Per-job timeout in seconds. `0` means fall back to the daemon config default. See [Timeouts](#timeouts). |
| `log_environment` | `bool` | When `true`, the full environment is dumped to the run log before execution. Defaults to `false`. |
| `created_at` | `DateTime<Utc>` | Timestamp of job creation. |
| `updated_at` | `DateTime<Utc>` | Timestamp of the last update to the job definition. |
| `last_run_at` | `Option<DateTime<Utc>>` | Timestamp of the most recent execution start, or `None` if never run. |
| `last_exit_code` | `Option<i32>` | Exit code from the most recent completed run, or `None` if never run. |
| `next_run_at` | `Option<DateTime<Utc>>` | Computed field. The scheduler calculates this at runtime based on the cron schedule and current time. Serialized in API responses but skipped during deserialization (`#[serde(skip_deserializing)]`), so it appears in `jobs.json` but is recalculated on load. |

### NewJob (Creation Payload)

When creating a job, the following fields are accepted:

- `name` (required)
- `schedule` (required)
- `execution` (required)
- `enabled` (optional, defaults to `true`)
- `timezone` (optional)
- `working_dir` (optional)
- `env_vars` (optional)
- `timeout_secs` (optional, defaults to `0`)
- `log_environment` (optional, defaults to `false`)

### JobUpdate (Partial Update Payload)

All fields in `JobUpdate` are optional. Only the fields present in the request body are modified; omitted fields remain unchanged. The `last_run_at` and `last_exit_code` fields are internal-only and cannot be set through the API (they are skipped during JSON deserialization).

---

## Execution Types

The `ExecutionType` enum determines how a job's command is built and spawned. It is serialized as a tagged JSON object with `type` and `value` fields.

### ShellCommand

Executes an inline shell command string.

**JSON representation:**
```json
{
  "type": "ShellCommand",
  "value": "echo hello world"
}
```

**Platform behavior:**

| Platform | Shell | Arguments |
|---|---|---|
| Windows | `cmd.exe` | `/C <command>` |
| Unix/macOS | `/bin/sh` | `-c <command>` |

### ScriptFile

Executes a script file by path.

**JSON representation:**
```json
{
  "type": "ScriptFile",
  "value": "deploy.sh"
}
```

**Platform behavior:**

| Platform | File Extension | Interpreter | Arguments |
|---|---|---|---|
| Windows | `.ps1` | `powershell.exe` | `-File <script>` |
| Windows | any other | `cmd.exe` | `/C <script>` |
| Unix/macOS | any | `/bin/sh` | `<script>` |

PowerShell detection on Windows is based on the `.ps1` file extension (case-insensitive).

---

## Cron Expressions

ACS uses the [`croner`](https://crates.io/crates/croner) crate for cron expression parsing and next-occurrence calculation. Both standard 5-field and extended 6-field formats are supported.

### 5-Field Format (Standard)

```
minute  hour  day-of-month  month  day-of-week
  *       *        *          *        *
```

### 6-Field Format (With Seconds)

```
second  minute  hour  day-of-month  month  day-of-week
  *       *       *        *          *        *
```

### Common Schedule Examples

| Expression | Description |
|---|---|
| `* * * * *` | Every minute |
| `*/5 * * * *` | Every 5 minutes |
| `*/15 * * * *` | Every 15 minutes |
| `0 * * * *` | Every hour (at minute 0) |
| `0 0 * * *` | Every day at midnight |
| `0 9 * * *` | Every day at 9:00 AM |
| `0 9 * * 1-5` | Every weekday at 9:00 AM |
| `0 0 * * 0` | Every Sunday at midnight |
| `0 0 1 * *` | First day of every month at midnight |
| `0 0 1 1 *` | January 1st at midnight (yearly) |
| `*/1 * * * *` | Every minute (explicit step) |
| `30 2 * * *` | Every day at 2:30 AM |

### Next Occurrence Calculation

The `compute_next_run` function calculates the next fire time after a given timestamp. The calculation is **exclusive** -- if the current time exactly matches a cron tick, the next occurrence after that tick is returned.

For example, with schedule `*/5 * * * *`:
- At 10:03, the next run is 10:05.
- At exactly 10:05, the next run is 10:10 (not 10:05 itself).

---

## Timezone Support

Jobs can be configured with an IANA timezone string (e.g., `"America/New_York"`, `"Europe/London"`, `"Asia/Tokyo"`). Timezone validation uses the `chrono_tz` crate.

### How Timezone Affects Scheduling

When a timezone is set:

1. The current UTC time is converted to the job's local timezone.
2. The cron expression is evaluated in that local timezone to find the next occurrence.
3. The resulting local time is converted back to UTC for the scheduler's internal tracking.

When no timezone is set (`None`), the cron expression is evaluated directly in UTC.

### DST (Daylight Saving Time) Handling

**Spring forward (clocks skip ahead):** If a scheduled time falls in the skipped gap (e.g., 2:30 AM during a spring-forward transition), the `croner` crate will either advance to the next valid time on that day or skip to the next day when that time exists again. Both behaviors are considered valid.

**Fall back (clocks repeat):** If a scheduled time falls in the repeated hour (e.g., 1:30 AM during a fall-back transition), the first occurrence (before the clock change) is used.

### Default

If no timezone is specified, all scheduling is performed in **UTC**.

---

## Job Lifecycle

A job progresses through the following stages:

```
Creation --> Scheduling --> Execution --> Completion
```

### Stage Details

1. **Creation**: A job is created via the CLI (`acs add`) or the REST API (`POST /api/jobs`). It is validated and persisted. The job defaults to `enabled: true`.

2. **Scheduling**: The scheduler continuously loads all enabled jobs, computes their next run times, and sleeps until the earliest one is due. When the job list changes (create, update, delete, enable, disable), the scheduler is woken via a `Notify` signal to re-evaluate immediately.

3. **Execution**: When a job's cron time arrives, the scheduler dispatches it to the executor. The executor:
   - Creates a `JobRun` record with `Running` status.
   - Broadcasts a `Started` event.
   - Optionally dumps the environment to the log (if `log_environment` is `true`).
   - Writes a command header (`$ <command>`) to the log.
   - Spawns the process with piped stdout/stderr.
   - Streams output to both the log store and the event broadcast channel.
   - Monitors for timeout and kill signals.

4. **Completion**: The run finishes with one of these terminal statuses. After completion, old run logs are cleaned up based on the configured retention limit (see [Configuration](configuration.md#field-reference)). Job metadata (`last_run_at`, `last_exit_code`) is updated automatically by a background task that listens for completion events.

### Run Status Transitions

| Status | Meaning | Trigger |
|---|---|---|
| `Running` | Execution is in progress. | Job spawned successfully. |
| `Completed` | Process exited (any exit code). | Process returned an exit status, including non-zero codes. Non-zero exit is **not** treated as `Failed`. |
| `Failed` | Infrastructure error prevented normal completion. | PTY spawn failure, process wait failure, task join error, or timeout. |
| `Killed` | Job was forcefully terminated. | Job deleted while running (`DELETE /api/jobs/{id}`), or daemon graceful shutdown. There is no dedicated kill endpoint. |

### JobRun Record

Each execution creates a `JobRun` with the following fields:

| Field | Type | Description |
|---|---|---|
| `run_id` | `Uuid` (v7) | Unique identifier for this run. |
| `job_id` | `Uuid` | The parent job's ID. |
| `started_at` | `DateTime<Utc>` | When execution began. |
| `finished_at` | `Option<DateTime<Utc>>` | When execution ended. `None` while running. |
| `status` | `RunStatus` | One of: `Running`, `Completed`, `Failed`, `Killed`. |
| `exit_code` | `Option<i32>` | Process exit code. Present only for `Completed` status. |
| `log_size_bytes` | `u64` | Total bytes written to the run log. |
| `error` | `Option<String>` | Error description for `Failed` or `Killed` runs. |

---

## Environment Variables

### Per-Job Environment Variables

Jobs can define custom environment variables via the `env_vars` field. These are injected into the spawned process alongside any inherited environment. Format is a key-value map:

```json
{
  "env_vars": {
    "DATABASE_URL": "postgres://localhost/mydb",
    "NODE_ENV": "production"
  }
}
```

On the CLI, environment variables are passed with the `-e` / `--env` flag. See [CLI Reference](cli-reference.md#acs-add) for details.

### log_environment Flag

When `log_environment` is set to `true`, the executor dumps the complete effective environment to the run log before executing the command. The output is formatted as:

```
=== Environment ===
HOME=/home/user
PATH=/usr/bin:/bin
DATABASE_URL=postgres://localhost/mydb
===================
```

This merges the inherited system environment with job-specific `env_vars` (job-specific variables override inherited ones with the same key). The entries are sorted alphabetically by key.

This flag is useful for debugging environment-sensitive issues.

---

## Timeouts

Timeouts control the maximum duration a job can run before being forcibly terminated.

### Resolution Order

1. **Per-job `timeout_secs`**: If the job's `timeout_secs` is greater than `0`, it is used.
2. **Daemon config `default_timeout_secs`**: If the job's `timeout_secs` is `0`, the daemon's `default_timeout_secs` from `DaemonConfig` is used.
3. **No timeout**: If both are `0`, the job runs without a time limit.

See [Configuration](configuration.md#field-reference) for the `default_timeout_secs` config field.

### Timeout Behavior

When a job exceeds its timeout:
- The run is terminated.
- The `JobRun` status is set to `Failed`.
- The `error` field is set to `"execution timed out"`.
- No exit code is recorded.
- A `Failed` event is broadcast.

---

## Working Directory

The optional `working_dir` field sets the current working directory for the spawned process. If not specified, the process inherits the daemon's working directory.

```json
{
  "working_dir": "/home/user/project"
}
```

---

## Job Validation

Both `NewJob` (creation) and `JobUpdate` (partial update) payloads are validated before being applied.

### Name Constraints

| Rule | Error |
|---|---|
| Name must not be empty or whitespace-only. | `"Job name cannot be empty"` |
| Name must not be a valid UUID string. | `"Job name cannot be a valid UUID"` |

The UUID restriction exists because jobs can be referenced by either name or ID in CLI commands and API calls; allowing UUID-format names would create ambiguity.

### Cron Expression Validation

The `schedule` field is parsed by `croner::Cron::from_str()`. If parsing fails, the error includes the invalid expression and the parser's error message:

```
Invalid cron expression '<expr>': <parser error>
```

### Timezone Validation

The `timezone` field (when provided) is parsed by `chrono_tz::Tz`. If parsing fails:

```
Invalid timezone '<tz>': <parser error>
```

### Update Validation

For `JobUpdate`, only the fields that are present (`Some`) are validated. Omitted (`None`) fields are not checked because they will not be changed.

For CLI usage examples, see [CLI Reference](cli-reference.md). For REST API endpoint details and examples, see [API Reference](api-reference.md).
