# Workflow Management

This document describes how workflows are defined, validated, scheduled, and executed in the Agent Cron Scheduler (ACS).

---

## Overview

A **Workflow** is the only top-level scheduling entity in ACS. It owns a cron schedule, an ordered list of steps, and all runtime configuration. There is no separate "Job" concept.

Each workflow run executes its steps in sequence. Steps can spawn subprocesses, make HTTP requests, branch based on a value, set named variables, or invoke an LLM agent. Data flows between steps via template substitution: any step field that supports templates can reference the trigger input or a prior step's output.

---

## Workflow Definition

### Full `Workflow` struct

| Field | Type | Default | Description |
|---|---|---|---|
| `id` | `Uuid` (v7) | auto | Unique identifier, auto-generated on creation. |
| `name` | `String` | required | Unique slug. Used to reference workflows in CLI commands and API calls. |
| `version` | `u32` | `1` | Auto-incremented when any definition-affecting field changes. Does not increment for `enabled` toggles. Always `>= 1`. |
| `schedule` | `String` | required | 5-field cron expression. Validated against the `croner` crate. |
| `timezone` | `Option<String>` | `null` | IANA timezone string. `null` means UTC. |
| `schedule_mode` | `ScheduleMode` | `"Cron"` | Controls how the scheduler handles ticks while a run is active. See [Schedule Modes](#schedule-modes). |
| `enabled` | `bool` | `true` | Whether the scheduler should run this workflow. |
| `steps` | `Vec<StepDef>` | required | Ordered list of steps. At least one required. Step ids must be unique across all steps (including nested match branches). |
| `default_input` | `Option<serde_json::Value>` | `null` | Baseline trigger payload used for cron-fired runs and manual triggers with no input. |
| `working_dir` | `Option<String>` | `null` | Default working directory for spawned processes. Overridden per step. |
| `env_vars` | `Option<HashMap<String, String>>` | `null` | Workflow-level environment variables injected into all spawned processes. |
| `allow_concurrent` | `bool` | `true` | Whether multiple runs of this workflow can execute simultaneously. |
| `on_failure` | `FailurePolicy` | `"abort"` | Default failure policy for steps that do not specify their own. |
| `last_run_at` | `Option<DateTime<Utc>>` | `null` | Timestamp of the most recent execution start. Set by the daemon. |
| `last_run_status` | `Option<RunStatus>` | `null` | Status of the most recent completed run. Set by the daemon. |
| `last_run_id` | `Option<Uuid>` | `null` | Run ID of the most recent run. Set by the daemon. |
| `next_run_at` | `Option<DateTime<Utc>>` | `null` | Computed at runtime; never persisted. Only present in GET responses. |
| `created_at` | `DateTime<Utc>` | auto | Timestamp of creation. |
| `updated_at` | `DateTime<Utc>` | auto | Timestamp of last update. |

### NewWorkflow (creation payload)

Required fields: `name`, `schedule`, `steps`.

Optional fields: `timezone` (defaults to the daemon-config `display_timezone`, which itself defaults to `America/Los_Angeles`), `schedule_mode`, `enabled` (defaults `true`), `default_input`, `working_dir` (defaults to `<user-documents>/agent-cron-scheduler/<sanitized-workflow-name>/`, created on disk if absent), `env_vars`, `allow_concurrent` (defaults `true` if omitted), `on_failure` (defaults `"abort"`).

### WorkflowUpdate (partial update payload)

All fields optional. Only fields present in the request body are modified. Omitted fields are unchanged.

Updatable: `name`, `schedule`, `timezone`, `schedule_mode`, `enabled`, `steps`, `default_input`, `working_dir`, `env_vars`, `allow_concurrent`, `on_failure`.

**Version bump rules.** Updating any of these fields increments `version` if the new value differs: `name`, `schedule`, `timezone`, `schedule_mode`, `steps`, `default_input`, `working_dir`, `env_vars`, `allow_concurrent`, `on_failure`. Toggling `enabled` does **not** bump the version.

Sending `null` for `timezone`, `working_dir`, or `default_input` in a PATCH body is treated the same as omitting the field — the existing value is unchanged. Send a non-null value to update them.

---

## Validation Rules

Validation runs on both `NewWorkflow` and `WorkflowUpdate` before any changes are applied.

### Name constraints

| Rule | Error message |
|---|---|
| Name must not be empty or whitespace-only. | `"Workflow name cannot be empty"` |
| Name must not be a valid UUID string. | `"Workflow name cannot be a valid UUID"` |

The UUID restriction prevents ambiguity when referencing workflows by name or ID in CLI commands and API routes.

### Cron expression validation

The `schedule` field is parsed by `croner::Cron::from_str`. If parsing fails:

```
Invalid cron expression '<expr>': <parser error>
```

### Timezone validation

The `timezone` field (when provided) is parsed by `chrono_tz::Tz`. If parsing fails:

```
Invalid timezone '<tz>': <parser error>
```

### Step constraints

- At least one step is required. (`"Workflow must have at least one step"`)
- Step `id` values must be unique across all steps in the workflow, including steps nested inside `MatchStep` branches and default branches. Uniqueness is enforced globally — a step `id` that appears both at the top level and inside a match branch will be rejected. (`"step id '<id>' is duplicated"`)
- `CaptureSpec.parser` (when set) must be one of `"json"`, `"lines"`, or `"raw"`. (`"Invalid capture parser '<v>': must be 'json', 'lines', or 'raw'"`)

### Update-only validation

For `WorkflowUpdate`, only the fields present (`Some(...)`) in the body are validated. Fields left `null`/absent are not checked.

---

## Steps

Each step is a variant of the `StepDef` enum. All variants share a flattened `StepDefCommon` struct (see [Common step fields](#common-step-fields)).

Steps are serialized as tagged JSON objects with a `"kind"` discriminant:

```json
{ "kind": "shell", "id": "fetch", "command": "curl https://example.com" }
```

### Shell

Executes an inline shell command string.

**Additional fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `command` | `String` | required | Shell command template. |
| `pass_stdin` | `bool` | `false` | If `true`, pipes the immediately-prior step's stdout to this process's stdin. The prior step is defined as the last step inserted into the execution context before this one — i.e., the step that ran immediately before this step in execution order. If the immediately-prior step produced no stdout (e.g. `set_var`, or a `match` with no consumed branch), nothing is piped — the search does not skip past it to find a non-empty earlier step. |

**Platform behavior:**

| Platform | Interpreter | Arguments |
|---|---|---|
| Unix/macOS | `sh` (resolved via PATH) | `-c <command>` |
| Windows | `cmd` (resolved via PATH) | `/C <command>` |

**Example:**

```json
{
  "kind": "shell",
  "id": "build",
  "command": "npm run build --prefix ${input.project_dir}",
  "timeout_secs": 120,
  "on_failure": "abort"
}
```

**Windows: shell-step stdout includes CRLF.** PowerShell's default output pipeline (e.g. `Get-Random`, `Write-Output`) appends a trailing `\r\n`. When a shell step's stdout is captured with `parser: null` (raw) and substituted into a downstream step's template via `${steps.<id>.stdout}`, the CRLF is injected verbatim — splitting a quoted argument mid-string and breaking the next step's command.

For example, `"command": "powershell -NoProfile -Command \"@('A','B') | Get-Random\""` produces stdout `A\r\n` (or `B\r\n`). Using `${steps.pick.stdout}` in the next step injects the CRLF into the command line.

Workaround: strip trailing whitespace with `.Trim()` — `"powershell -NoProfile -Command \"(@('A','B') | Get-Random).Trim()\""`. This applies to any shell step whose output you template into another step's args. On Unix, `echo` adds a trailing `\n`; mitigate with `echo -n` or `tr -d '\r\n'`.

---

### Script

Executes a script file via an explicit or inferred interpreter.

**Additional fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `path` | `String` | required | Path to the script file. Supports template substitution. |
| `script_type` | `Option<String>` | `null` | Interpreter to use. One of `"shell"`, `"batch"`, `"python"`, `"powershell"`. When `null`, defaults to `"shell"` behavior. |
| `args` | `Option<String>` | `null` | Whitespace-separated arguments passed to the script. Supports template substitution. |
| `pass_stdin` | `bool` | `false` | Pipe the immediately-prior step's stdout to this process's stdin. The prior step is defined as the last step inserted into the execution context before this one — i.e., the step that ran immediately before this step in execution order. If the immediately-prior step produced no stdout (e.g. `set_var`, or a `match` with no consumed branch), nothing is piped — the search does not skip past it to find a non-empty earlier step. |

**Interpreter selection:**

| `script_type` | Unix interpreter | Windows interpreter |
|---|---|---|
| `null` or `"shell"` | `sh <path> [args]` (resolved via PATH) | `cmd /C <path> [args]` (resolved via PATH) |
| `"batch"` | Error (Windows only) | `cmd /C <path> [args]` (resolved via PATH) |
| `"python"` | `python3 <path> [args]` | `python <path> [args]` |
| `"powershell"` | `pwsh -File <path> [args]` | `pwsh -File <path> [args]` if `pwsh` is on PATH; otherwise `powershell.exe -File <path> [args]` |

On Windows, the PowerShell interpreter is selected at step execution time: `pwsh` (PowerShell 7+ Core) is tried first via a PATH check; if not found, `powershell.exe` (Windows PowerShell 5.1) is used as a fallback. On Unix, `pwsh` is always used.

An unknown `script_type` produces a `StepError::Internal` error and the run fails immediately.

**Example:**

```json
{
  "kind": "script",
  "id": "deploy",
  "path": "${input.scripts_dir}/deploy.sh",
  "script_type": "shell",
  "args": "--env ${input.environment}",
  "timeout_secs": 300
}
```

---

### Http

Makes an outbound HTTP request using `reqwest`.

**Additional fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `method` | `String` | required | HTTP method. One of `GET`, `POST`, `PUT`, `PATCH`, `DELETE`, `HEAD`, `OPTIONS`, `TRACE`, `CONNECT` (case-insensitive). |
| `url` | `String` | required | Request URL. Supports template substitution. |
| `headers` | `HashMap<String, String>` | `{}` | Request headers. Values support template substitution. |
| `body` | `Option<String>` | `null` | Request body. Supports template substitution. |
| `expect_status` | `Vec<u16>` | `[200..299]` | HTTP status codes treated as success (exit code 0). All others produce exit code equal to the HTTP status code. Defaults to the range 200–299 if the list is empty. |

**Response handling.** The response body is written to the run log. `stdout` on the `StepOutput` is parsed as follows: if the response `Content-Type` header contains `"application/json"`, the body is parsed as JSON (falls through to capture parser on parse failure). Otherwise, the capture parser spec is applied.

**Exit code semantics.** Unlike shell steps, the exit code for an HTTP step is not from a subprocess. If the response status code is in `expect_status`, exit code is `0`. Otherwise, exit code equals the numeric HTTP status code (e.g., `404`). A non-zero exit code is treated as step failure by the executor, subject to the step's `on_failure` policy.

An unknown HTTP method produces a `StepError::Internal` error.

**Example:**

```json
{
  "kind": "http",
  "id": "notify",
  "method": "POST",
  "url": "https://hooks.example.com/deploy",
  "headers": {
    "Authorization": "Bearer ${input.api_token}",
    "Content-Type": "application/json"
  },
  "body": "{\"repo\": \"${input.repo}\", \"sha\": \"${steps.build.exports.sha}\"}",
  "expect_status": [200, 201, 204],
  "timeout_secs": 10
}
```

---

### Match

Multi-way branching step. Evaluates a template expression and dispatches to one of several named branches.

**Additional fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `expr` | `String` | required | Template expression that evaluates to a string. Used for exact-match comparison against case keys. |
| `cases` | `HashMap<String, Vec<StepDef>>` | required | Named branches. Keys are exact-match strings; values are sequences of steps to execute if the expression matches. |
| `default` | `Option<Vec<StepDef>>` | `null` | Fallthrough branch executed when `expr` does not match any case key. If absent and no case matches, the match step is a no-op. |

**Runtime behavior.** The `MatchStep` does not spawn a process. The executor evaluates `expr`, looks up the matching case, and recursively executes that branch's steps in the same execution context as the parent workflow. Chosen branch steps appear sequentially in the run's `steps[]` list after the synthetic `MatchStep` entry.

The `MatchStep` itself always records a `RunStatus::Completed` `StepRun` entry. The selected branch is observable via the subsequent step records: when case `"A"` is taken, only `cases.A` steps appear in `steps[]` after the synthetic `MatchStep` entry; when no case matches and a default is configured, only the default branch steps appear; when neither matches, no branch steps appear at all.

Step ids must be globally unique — ids inside branch steps are validated against the top-level step ids and against ids in all other branches.

**Example:**

```json
{
  "kind": "match",
  "id": "route",
  "expr": "${steps.check.exports.env}",
  "cases": {
    "production": [
      { "kind": "shell", "id": "deploy-prod", "command": "deploy.sh --env production" }
    ],
    "staging": [
      { "kind": "shell", "id": "deploy-staging", "command": "deploy.sh --env staging" }
    ]
  },
  "default": [
    { "kind": "shell", "id": "skip-deploy", "command": "echo 'Unknown env, skipping'" }
  ]
}
```

---

### SetVar

Pure context mutation. No subprocess is spawned. Computes named exports from template expressions and makes them available to subsequent steps via `${steps.<id>.exports.<name>}`.

**Additional fields:**

| Field | Type | Description |
|---|---|---|
| `exports` | `HashMap<String, String>` | Map of export name to template value string. |

**Export value parsing.** After template substitution, each resolved value string is first attempted as JSON (`serde_json::from_str`). If that succeeds, the parsed `serde_json::Value` is stored directly (so `"42"` becomes a JSON Number, `"\"hello\""` becomes a String without extra quotes, `"{\"k\":1}"` becomes an Object). If JSON parsing fails, the raw string is stored as `Value::String`. This maximizes chaining utility.

This step always exits with code `0` and emits `stdout: null`.

**Example:**

```json
{
  "kind": "set_var",
  "id": "params",
  "exports": {
    "full_name": "${input.first} ${input.last}",
    "threshold": "100",
    "config": "{\"retries\": 3}"
  }
}
```

---

### Agent

First-class invocation of an LLM agent runtime. Currently supports `claude_code_cli`; the architecture is extensible to additional agent types.

**Additional fields:**

| Field | Type | Default | Description |
|---|---|---|---|
| `agent_type` | `AgentType` | required | `"claude_code_cli"` is the only current value. Serializes as a snake_case string. |
| `prompt` | `String` | required | Prompt to send to the agent. Supports `${input.*}` and `${steps.*}` template substitution. |
| `model` | `Option<String>` | `null` | Optional Claude model identifier. Passed as `--model <value>` to the `claude` CLI. Example: `"claude-haiku-4-5-20251001"`. `None` = use whatever model `claude` defaults to. |
| `extra_args` | `Vec<String>` | `[]` | Additional command-line arguments appended verbatim to the `claude` invocation as separate argv elements. Useful for flags like `--allowedTools <tool>`, `--resume <session>`, or `--model-config <path>`. No shell parsing or escaping is applied — each item becomes one argv element. |

**Canonical argv baseline for `claude_code_cli`:**

The `claude_code_cli` runner builds argv directly — there is no template string to override. The canonical baseline argv is:

```
claude -p <resolved_prompt> --output-format stream-json --verbose --dangerously-skip-permissions
```

If `model` is set, `--model <value>` is inserted. If `extra_args` is non-empty, each item is appended verbatim as a separate argv element. The process is spawned directly (no `cmd /C` or `sh -c` wrapper), so shell-escaping concerns do not apply.

**Prompt substitution.** The `prompt` field is rendered through the standard `${input.*}` / `${steps.<id>.stdout}` / `${steps.<id>.exports.<name>}` template substitution before being passed as a single argv element. Because the prompt is passed as one discrete argv argument (no shell), any character — including `?`, commas, quotes, newlines, backslashes — is delivered verbatim to `claude`.

**Cost tracking.** Agent steps stream Claude's NDJSON output through a `ClaudeStreamParser`. The parser extracts `total_cost_usd`, `duration_ms`, `num_turns`, `model`, and `usage` from `result` events and accumulates them across multiple invocations. Costs from agent steps are summed into `WorkflowRun.total_cost_usd`. Only `AgentStep` produces cost data; a `ShellStep` that happens to call `claude -p` directly does not, and the legacy stdin-fed pattern `echo "prompt" | claude --output-format stream-json` (note: no `-p` flag) still runs fine as a shell step but is **not** auto-rewritten into an `AgentStep` and does **not** get first-class cost tracking. Convert it to an `AgentStep` (or a `ShellStep` calling `claude -p "..."`) to participate in cost aggregation.

**stdout.** The `result` field from the last Claude `result` event becomes `StepOutput.stdout` (as `Value::String`). If no structured result is present (no `result` event in the output), the raw captured output is parsed per the step's `capture.parser` as a fallback (so the stdout `Value` can be non-string depending on the capture spec).

**Example:**

```json
{
  "kind": "agent",
  "id": "review",
  "agent_type": "claude_code_cli",
  "prompt": "Review the diff at ${input.diff_url} and summarize issues.",
  "timeout_secs": 300,
  "on_failure": "abort"
}
```

**Advanced — session resume with `extra_args`:**

```json
{
  "kind": "agent",
  "agent_type": "claude_code_cli",
  "prompt": "Continue the analysis from the prior session.",
  "model": "claude-haiku-4-5-20251001",
  "extra_args": ["--resume", "${steps.start.exports.session_id}"]
}
```

Each `extra_args` item is template-substituted before being passed as a discrete argv element, so `${steps.*}` references work as expected.

---

## Common Step Fields

All step variants flatten `StepDefCommon` at the top level of their JSON object. There is no nested `"common"` key.

| Field | Type | Default | Description |
|---|---|---|---|
| `id` | `String` | required | Stable step identifier. Used for template references and run records. Must be unique across all steps (including nested match branches). |
| `on_failure` | `Option<FailurePolicy>` | `null` | Failure policy for this step. `null` means inherit the workflow's `on_failure`. |
| `always_run` | `bool` | `false` | If `true`, this step executes even after an earlier step has aborted the run. Useful for cleanup steps. |
| `timeout_secs` | `Option<u64>` | `null` | Per-step timeout in seconds. `null` or `0` means no timeout. When a step times out, it returns `StepError::Timeout` and the run is marked `Failed`. |
| `working_dir` | `Option<String>` | `null` | Overrides the workflow's `working_dir` for this step. |
| `env_vars` | `Option<HashMap<String, String>>` | `null` | Merged with `workflow.env_vars`; step-level values win on key collision. |
| `capture` | `CaptureSpec` | see below | Controls stdout capture. |

---

## CaptureSpec

Controls how a step's stdout is captured and stored.

| Field | Type | Default | Description |
|---|---|---|---|
| `stdout_max_bytes` | `usize` | `65536` (64 KB) | Maximum bytes of stdout to retain in the capture buffer. Output written to the log is not truncated — truncation applies only to the `StepOutput.stdout` field stored in memory. |
| `parser` | `Option<String>` | `null` | How to interpret the captured bytes. One of `"json"`, `"lines"`, `"raw"`, or `null`. `null` is equivalent to `"raw"`. |

**Parser behavior:**

| Parser | Behavior |
|---|---|
| `null` or `"raw"` | Raw bytes decoded as UTF-8 (lossy). Stored as `Value::String`. |
| `"json"` | Attempt to parse the captured bytes as JSON. On success, stored as the parsed value. On failure, falls back to `"raw"` behavior. |
| `"lines"` | Split the output on `\n`, filtering empty lines. Stored as `Value::Array` of `Value::String` entries. |

An invalid parser string (anything other than `"json"`, `"lines"`, `"raw"`) is rejected at workflow creation/update time.

**Example:**

```json
{
  "kind": "shell",
  "id": "list-files",
  "command": "ls -1 /data",
  "capture": {
    "stdout_max_bytes": 16384,
    "parser": "lines"
  }
}
```

---

## Template Substitution

Fields documented as "template" in step definitions support `${...}` substitution before execution. Template substitution is **single-pass** — references are not recursively expanded.

### Supported namespaces

**`input.<dotted.path>`** — references the trigger input payload.

Supports dotted paths for nested fields. The dotted path is converted to a JSON Pointer for lookup.

```
${input.repo}                   → top-level field "repo"
${input.user.email}             → nested field at user.email
${input.options.format}         → nested field at options.format
```

**`steps.<step_id>.<accessor>`** — references a prior step's output.

Accessors:
- `stdout` — the step's stdout value. Strings are rendered raw; structured values are serialized as compact JSON.
- `exit_code` — the step's exit code as a decimal string.
- `exports.<name>` — a named export value from a `SetVarStep`.

```
${steps.fetch.stdout}                     → full stdout of step "fetch"
${steps.fetch.exit_code}                  → exit code of step "fetch"
${steps.params.exports.session_id}        → named export "session_id" from step "params"
```

### Missing reference behavior

A reference that cannot be resolved (step not yet run, field not in input, unknown accessor) produces an empty string and a warning in the run log. The step command receives the empty string; this is not an error by itself but may cause the command to behave unexpectedly.

### String rendering

- `Value::String` renders as the raw string content (no JSON quotes).
- All other `serde_json::Value` types render as compact JSON.
- Numbers render as their JSON representation (e.g., `42`, `3.14`).

### Where templates are supported

- `ShellStep.command`
- `ScriptStep.path`, `ScriptStep.args`
- `HttpStep.url`, `HttpStep.headers` (values), `HttpStep.body`
- `MatchStep.expr`
- `SetVarStep.exports` (values)
- `AgentStep.prompt` (supports `${input.*}`, `${steps.*}`)
- `AgentStep.extra_args[]` — each item supports `${input.*}`, `${steps.*}`

---

## Run Lifecycle

### Trigger sources

**Cron-fired run.** The scheduler computes the workflow's next fire time from its cron expression and timezone. When the time arrives, a run is dispatched with `trigger.input = null`, which resolves to `workflow.default_input` (or empty `{}` if unset).

**Manual trigger.** A run is dispatched via `POST /api/workflows/{id}/trigger` or `acs workflows trigger`. The request body is a `TriggerParams` object.

### TriggerParams

```json
{
  "input": <any JSON value or null>,
  "env": { "KEY": "VALUE" },
  "target_step": "step-id"
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `input` | `serde_json::Value` | `null` | Replaces `default_input` for this run. If `null` (or omitted), `default_input` is used. This is a full replacement — the two values are not merged. |
| `env` | `Option<HashMap<String, String>>` | `null` | Overlaid onto `workflow.env_vars`. Trigger env wins on key collision. |
| `target_step` | `Option<String>` | `null` | Route the trigger's `input` to a specific step's stdin. When set to a step `id`, the trigger's `input` value is serialized and written to that step's stdin when it executes. Strings are written as raw bytes; all other JSON values are serialized as compact JSON. Only `Shell` and `Script` steps consume stdin — other step kinds ignore this value. An `id` that does not match any step is also ignored silently. `target_step` overrides `pass_stdin` for the matching step. |

### Input resolution

At run start, the effective input is determined once and frozen for the entire run:

1. If `trigger.input` is non-null, use `trigger.input`.
2. Otherwise, use `workflow.default_input`.
3. If `workflow.default_input` is also null, the effective input is `{}`.

### Environment variable resolution

| Priority | Source | Notes |
|---|---|---|
| 1 (lowest) | Inherited environment | System environment inherited by the daemon process. |
| 2 | `workflow.env_vars` | Per-workflow environment. |
| 3 (highest) | `trigger.env` | Per-trigger overlay. Only present for triggers that include an explicit `env` map. |

Step-level `env_vars` are merged into the resolved context before spawning (step wins on conflict with workflow-level env).

### Workflow snapshot

At trigger time, the full `Workflow` definition is copied into `WorkflowRun.workflow_snapshot`. Runs are fully self-contained — audit, replay, and reads of in-flight runs do not depend on the on-disk workflow definition. If a workflow is updated mid-flight, the running run uses the snapshot from its trigger time.

---

## Failure Policies

`FailurePolicy` determines what happens when a step's exit code is non-zero or the step returns a `StepError`.

The workflow-level `on_failure` is the default for all steps. A step's own `on_failure` (if non-null) overrides the workflow default for that step only.

### Abort (default)

```json
"on_failure": "abort"
```

On failure, the run is marked as `Failed`. Subsequent steps are **not** executed, with one exception: steps with `always_run: true` run regardless.

### Continue

```json
"on_failure": "continue"
```

On failure, the step is recorded with status `Failed` but execution continues with the next step. The failed step's output (including `exit_code`) is still captured into the step context, so downstream templates like `${steps.<id>.exit_code}` resolve correctly. The run's final status is `Completed` as long as no `Abort`-policy step also failed.

### Retry

```json
"on_failure": { "retry": { "attempts": 3, "backoff_ms": 1000 } }
```

The step is retried up to `attempts` times. Between each attempt, the executor sleeps `backoff_ms` milliseconds. A step that exits with a non-zero code is retried. `StepError::Killed` is never retried — kill is always terminal. If all attempts are exhausted, the step is treated as `Abort` and the run is marked `Failed`.

Only the final attempt's outcome is recorded in `step_runs`. Per-attempt records are not stored.

### `always_run` cleanup

Steps with `always_run: true` execute even when the run has aborted (due to an `Abort`-policy failure or a kill signal). This allows cleanup steps (e.g., removing temp files, notifying on failure) to run at the end of a failed run. If the run was killed, `always_run` steps still execute after the kill.

---

## Concurrency and Timeouts

### `allow_concurrent`

`Workflow.allow_concurrent` defaults to `true`. When `true`, multiple runs of the same workflow can execute simultaneously — there is no cap on concurrent runs.

When `allow_concurrent: false`, the workflow rejects new runs while a run is active. The guard applies to all trigger sources:

- **HTTP trigger** (`POST /api/workflows/{id}/trigger`): returns `409 Conflict` with the body `{ "error": "concurrent_run_active", "message": "Workflow already has a running run; concurrent runs are disabled.", "active_run_id": "<run_id>" }`. No new run is created and the active run is left untouched.
- **Cron tick**: the dispatch is skipped and a warning is logged (`workflow {name} has active run, skipping dispatch (allow_concurrent=false)`). The active run is left untouched and the next eligible cron tick after it finishes will dispatch a new run.

`schedule_mode: "WaitForCompletion"` is independent of `allow_concurrent`. When set, cron ticks are skipped while a run is active for that workflow regardless of the `allow_concurrent` value. HTTP triggers are unaffected by `schedule_mode` — they only check `allow_concurrent`.

### Timeouts

Timeouts are per-step only. There is no workflow-level aggregate timeout.

`StepDefCommon.timeout_secs` controls the per-step timeout. A value of `null` or `0` means no timeout. When a step exceeds its timeout:

- The subprocess (or HTTP request) is killed via `process_kill::kill_process_tree`.
- The step returns `StepError::Timeout(secs)`.
- The step's `StepRun.status` is set to `Failed` with an error message of `"timeout after <N> seconds"`.
- The run is marked `Failed` (subject to the step's failure policy).

### Killing a run

Sending `POST /api/runs/{run_id}/kill` signals the currently executing step to terminate. The kill signal propagates through a `tokio::sync::watch<bool>` channel. Each subprocess step (`Shell`, `Script`, `Agent`) races its read loop against the kill signal; on receipt it calls `kill_process_tree(pid)`. `HttpStep` cancels the in-flight request. `SetVarStep` and `MatchStep` are synchronous and do not need a kill path.

After a kill, steps with `always_run: true` still execute. The final run status is `Killed`.

Killed runs emit a `RunFailed` SSE event. The persistent `status` is `Killed`.

---

## Schedule Modes

`ScheduleMode` controls how the scheduler handles cron ticks while a run is active.

`ScheduleMode` serializes as PascalCase strings (`"Cron"`, `"WaitForCompletion"`), unlike `FailurePolicy` which uses snake_case.

### `Cron` (default)

The scheduler fires on every cron tick. Combined with `allow_concurrent: true`, multiple runs of the same workflow can execute in parallel. Combined with `allow_concurrent: false`, the cron tick is skipped if a run is already active for the workflow; the active run is left untouched and the next eligible tick after it finishes will dispatch a new run.

### `WaitForCompletion`

When `schedule_mode` is `"WaitForCompletion"`, the scheduler skips any cron tick that fires while a run is active for that workflow. The workflow becomes eligible again only after the current run completes, and the next natural cron tick triggers a new run.

Use this mode for long-running workflows where overlapping or back-to-back executions would be harmful and where missing an intermediate tick is preferable to queueing or killing work in progress.

---

## Run Records

### `WorkflowRun`

Each execution creates a `WorkflowRun` row in the `workflow_runs` table of `<data_dir>/acs.db` (SQLite). The full `Workflow` snapshot, the `StepRun` list, and the trigger input are stored as JSON-encoded columns; everything else is a typed column. See [`storage.md`](./storage.md) for the schema.

| Field | Type | Description |
|---|---|---|
| `run_id` | `Uuid` (v7) | Unique run identifier. |
| `workflow_id` | `Uuid` | The parent workflow's ID. |
| `workflow_version` | `u32` | The workflow version at trigger time. |
| `workflow_snapshot` | `Workflow` | Full workflow definition snapshotted at trigger time. |
| `started_at` | `DateTime<Utc>` | When execution began. |
| `finished_at` | `Option<DateTime<Utc>>` | When execution ended. `null` while running. |
| `status` | `RunStatus` | One of `Running`, `Completed`, `Failed`, `Killed`. |
| `trigger_input` | `Option<serde_json::Value>` | The effective input used for this run (after `default_input` vs trigger replacement). `null` if the input was empty. |
| `steps` | `Vec<StepRun>` | Execution-order list of step run records. |
| `total_cost_usd` | `Option<f64>` | Sum of `cost_usd` across all `AgentStep` runs. `null` if no agent steps ran. |
| `total_input_tokens` | `u64` (default `0`) | Sum of input tokens across agent steps in the run; non-agent runs contribute 0. |
| `total_output_tokens` | `u64` (default `0`) | Sum of output tokens across agent steps in the run; non-agent runs contribute 0. |
| `total_duration_ms` | `Option<u64>` | Wall-clock duration from `started_at` to `finished_at`. |

### `StepRun`

| Field | Type | Description |
|---|---|---|
| `step_index` | `usize` | 0-based position in the runtime execution sequence. The first executed step has `step_index: 0`, the second has `step_index: 1`, and so on. Branch steps inside a `MatchStep` continue the counter from where the match step left off. Matches the `step_index` carried in `StepStarted` / `StepCompleted` SSE events and the `step_index` query parameter on `GET /api/runs/{run_id}/log`. |
| `step_id` | `String` | Matches `StepDefCommon.id`. |
| `kind` | `String` | `"shell"`, `"script"`, `"http"`, `"match"`, `"set_var"`, or `"agent"`. |
| `status` | `RunStatus` | `Running`, `Completed`, `Failed`, or `Killed`. |
| `started_at` | `DateTime<Utc>` | When the step began. |
| `finished_at` | `Option<DateTime<Utc>>` | When the step ended. |
| `exit_code` | `Option<i32>` | Process exit code (shell/script/agent steps). `null` for non-process steps or on kill/timeout. |
| `log_byte_offset_start` | `u64` | Byte offset in the run's combined log file where this step's START marker begins. |
| `log_byte_offset_end` | `Option<u64>` | Byte offset just after this step's END marker. Populated for `Completed`, `Failed`, `Killed`, and timed-out steps — every subprocess step writes the END marker before surfacing its error. `null` only when the step errored before `write_step_end` ran (e.g. a spawn failure with no run loop, or template-substitution failure before START). For a synthetic `MatchStep` record, the offsets are patched to span its child branch; if no children ran, end stays `null` (the slice endpoint treats `null` as "tail to EOF"). |
| `cost_usd` | `Option<f64>` | Cost in USD extracted from the agent's streaming output. Present for `AgentStep` only. |
| `error` | `Option<String>` | Error description for `Failed` or `Killed` steps. |

Per-step stdout/stderr is captured on disk in the combined run log file (see [`storage.md`](./storage.md)). Each `StepRun`'s `log_byte_offset_*` pair frames its slice; fetch the bytes via `GET /api/runs/{run_id}/log?step_index=N`.

### Run status values

`RunStatus` is serialized in PascalCase.

| Status | Meaning |
|---|---|
| `Running` | Run is in flight. |
| `Completed` | Run reached the end of its steps. All `Abort`-policy steps succeeded; `Continue`-policy steps may have failed without terminating the run. |
| `Failed` | Run terminated early due to a step abort, timeout, spawn failure, or other infrastructure error. |
| `Killed` | Externally terminated via `POST /api/runs/{id}/kill` or daemon shutdown. |

### Log file structure

One combined log file per run:

```
<data_dir>/logs/<workflow_id>/<run_id>.log
```

Each step's output is wrapped in marker lines:

```
===== ACS-<VERSION>:STEP:<step_id>:START:<iso8601> =====
<stdout/stderr interleaved>
===== ACS-<VERSION>:STEP:<step_id>:END:exit=<code or -1>:<iso8601> =====
```

`StepRun.log_byte_offset_start` records the byte offset of the START marker's first byte. `StepRun.log_byte_offset_end` records the byte offset just after the END marker. These offsets allow the UI to efficiently seek to a specific step's output without scanning the full log.

The daemon version is embedded in markers (`ACS-<VERSION>`) to allow log parsers to handle format evolution.

---

## Cron Expressions

ACS uses the [`croner`](https://crates.io/crates/croner) crate for parsing and next-occurrence calculation. Standard **5-field** cron syntax is used.

```
minute  hour  day-of-month  month  day-of-week
  *       *        *          *        *
```

### Common expressions

| Expression | Description |
|---|---|
| `* * * * *` | Every minute |
| `*/5 * * * *` | Every 5 minutes |
| `0 * * * *` | Every hour (at minute 0) |
| `0 9 * * *` | Every day at 9:00 AM |
| `0 9 * * 1-5` | Weekdays at 9:00 AM |
| `0 0 * * 0` | Every Sunday at midnight |
| `0 0 1 * *` | First day of every month at midnight |
| `30 2 * * *` | Every day at 2:30 AM |

### Next occurrence calculation

The scheduler computes the next fire time as the **first occurrence strictly after** the current time. If the current time exactly matches a cron tick, the next occurrence after that tick is returned (exclusive semantics).

### Timezone support

Timezone strings must be valid IANA timezone identifiers (e.g., `"America/New_York"`, `"Europe/London"`, `"Asia/Tokyo"`) parsed by the `chrono-tz` crate. When set, the cron expression is evaluated in the workflow's local timezone.

DST transitions: if a scheduled time falls in a spring-forward gap, `croner` advances to the next valid time. If a scheduled time falls in a fall-back repeat, either occurrence may be returned.

When `timezone` is `null`, all scheduling is performed in UTC.

---

## Examples

### Simple two-step pipeline

A workflow that fetches a report URL from input, then notifies Slack:

```json
{
  "name": "daily-report",
  "schedule": "0 8 * * 1-5",
  "timezone": "America/New_York",
  "default_input": {
    "report_url": "https://internal.example.com/reports/daily"
  },
  "on_failure": "abort",
  "steps": [
    {
      "kind": "http",
      "id": "fetch-report",
      "method": "GET",
      "url": "${input.report_url}",
      "expect_status": [200],
      "capture": { "parser": "json" },
      "timeout_secs": 15
    },
    {
      "kind": "http",
      "id": "notify-slack",
      "method": "POST",
      "url": "https://hooks.slack.com/services/T00000000/B00000000/XXXXXXXX",
      "headers": { "Content-Type": "application/json" },
      "body": "{\"text\": \"Report fetched. Status: ${steps.fetch-report.exit_code}\"}"
    }
  ]
}
```

### Conditional deployment with cleanup

A workflow that builds, checks the result, deploys to the right environment, and always runs a cleanup step:

```json
{
  "name": "deploy-pipeline",
  "schedule": "0 2 * * *",
  "on_failure": "abort",
  "steps": [
    {
      "kind": "shell",
      "id": "build",
      "command": "make build",
      "timeout_secs": 300
    },
    {
      "kind": "set_var",
      "id": "set-env",
      "exports": {
        "target": "\"production\""
      }
    },
    {
      "kind": "match",
      "id": "deploy-route",
      "expr": "${steps.set-env.exports.target}",
      "cases": {
        "production": [
          {
            "kind": "shell",
            "id": "deploy-prod",
            "command": "deploy.sh --env production",
            "timeout_secs": 600
          }
        ],
        "staging": [
          {
            "kind": "shell",
            "id": "deploy-staging",
            "command": "deploy.sh --env staging",
            "timeout_secs": 300
          }
        ]
      }
    },
    {
      "kind": "shell",
      "id": "cleanup",
      "command": "rm -rf /tmp/build-artifacts",
      "always_run": true
    }
  ]
}
```

### Agentic pipeline (pre-process, agent, post-process)

A workflow that preprocesses input, invokes Claude to analyze it, then posts the result:

```json
{
  "name": "code-review",
  "schedule": "0 */4 * * *",
  "default_input": {
    "repo": "acme/backend",
    "branch": "main"
  },
  "steps": [
    {
      "kind": "shell",
      "id": "fetch-diff",
      "command": "git -C /repos/${input.repo} diff HEAD~1 HEAD --stat",
      "capture": { "parser": "raw" },
      "timeout_secs": 30
    },
    {
      "kind": "agent",
      "id": "analyze",
      "agent_type": "claude_code_cli",
      "prompt": "Review this diff for the ${input.repo} repository on branch ${input.branch}. Diff stats:\n\n${steps.fetch-diff.stdout}\n\nProvide a concise summary of changes and flag any potential issues.",
      "timeout_secs": 120,
      "on_failure": "abort"
    },
    {
      "kind": "http",
      "id": "post-review",
      "method": "POST",
      "url": "https://api.example.com/reviews",
      "headers": {
        "Authorization": "Bearer ${input.api_token}",
        "Content-Type": "application/json"
      },
      "body": "{\"repo\": \"${input.repo}\", \"summary\": \"${steps.analyze.stdout}\"}",
      "expect_status": [200, 201]
    }
  ]
}
```

---

## CLI Reference

The `acs workflows` subcommand manages workflows via the daemon API.

| Command | Description |
|---|---|
| `acs workflows list [--enabled] [--disabled] [--json]` | List all workflows. Filter by enabled/disabled status. |
| `acs workflows get <id-or-name> [--json]` | Show one workflow by UUID or name. |
| `acs workflows create --file <path>` | Create a workflow from a JSON file (`NewWorkflow` shape). |
| `acs workflows create --json '<json>'` | Create a workflow from inline JSON. |
| `acs workflows update <id-or-name> --file <path>` | Update a workflow from a JSON file (`WorkflowUpdate` shape). |
| `acs workflows update <id-or-name> --json '<json>'` | Update a workflow from inline JSON. |
| `acs workflows update <id-or-name> --enable` | Convenience: set `enabled = true`. |
| `acs workflows update <id-or-name> --disable` | Convenience: set `enabled = false`. |
| `acs workflows delete <id-or-name> [-y]` | Delete a workflow. Prompts for confirmation unless `-y` is passed. |
| `acs workflows trigger <id-or-name> [--input '<json>'] [--input-file <path>] [-e KEY=VALUE] [--target-step <id>] [--follow]` | Trigger a manual run. `--follow` streams SSE events until the run completes or fails. |
| `acs workflows runs <id-or-name> [--limit N] [--offset N] [--json]` | List runs for a workflow. Latest-first. Default limit 20, max 100. |

Workflows can be referenced by UUID or name in all commands. `--file` and `--json` are mutually exclusive.

For full details see [CLI Reference](cli-reference.md). For REST endpoint details see [API Reference](api-reference.md). For on-disk storage see [Storage](storage.md).
