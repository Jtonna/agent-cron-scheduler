# integration-tests

External, end-to-end integration tests for the agent-cron-scheduler (ACS)
daemon. These tests talk to a **running** daemon over HTTP — they are not
`cargo test` or `vitest` tests. The same scripts work against any reachable
daemon URL (local dev, a CI ephemeral instance, a staging box, etc.).

## What's in here

```
integration-tests/
  README.md
  run-test.ps1            # Windows / PowerShell wrapper
  run-test.sh             # Unix / bash wrapper
  _runner.py              # actual test logic (Python 3.8+ stdlib only)
  workflows/
    weather-greeter-demo.json   # seed workflow definition
```

## Prerequisites

- A running ACS daemon. Default URL: `http://127.0.0.1:8377`.
- Python 3.8+ on `PATH` (`python3` on Unix, `python`/`python3`/`py` on Windows).
- For workflows that use `agent` steps (like `weather-greeter-demo`): the
  daemon must be able to invoke the `claude` CLI, which means an active
  Anthropic API session / credential is required on the daemon host.
- Outbound internet (the seed workflow calls `api.open-meteo.com`).

No package install step. Stdlib only.

## How to run

From the repo root.

Windows (PowerShell):

```powershell
.\integration-tests\run-test.ps1
```

Unix (bash):

```bash
./integration-tests/run-test.sh
```

The default invocation creates the `weather-greeter-demo` workflow, triggers
it with its `default_input` (mood=happy), polls until completion, asserts
every step succeeded, prints a step summary plus the agent's final text,
then deletes the workflow.

Expected runtime: roughly 20-40 seconds. Most of that is the `agent` step
waiting on the Claude CLI.

### Flags

| flag | default | notes |
| --- | --- | --- |
| `--daemon-url <url>` | `http://127.0.0.1:8377` | base URL of the daemon |
| `--workflow <path>` | `workflows/weather-greeter-demo.json` | workflow JSON to load |
| `--input '<json>'` | (none) | trigger input override; must be a JSON object. e.g. `'{"mood":"tired"}'` |
| `--keep` | off | skip the final DELETE — useful for inspecting the run in the UI |
| `--timeout-secs <n>` | `180` | how long to poll before giving up |

### Examples

```bash
# Run against a non-default daemon
./integration-tests/run-test.sh --daemon-url http://127.0.0.1:9000

# Force the "tired" branch
./integration-tests/run-test.sh --input '{"city":"Fresno","lat":"36.7378","lon":"-119.7871","mood":"tired"}'

# Keep the workflow around to poke at it in the UI afterward
./integration-tests/run-test.sh --keep
```

## What the test asserts

1. `POST /api/workflows` returns a workflow id.
2. `POST /api/workflows/:id/trigger` returns a `run_id`.
3. `GET /api/runs/:run_id` eventually shows `status == "Completed"` within
   the timeout (polled every 2 seconds, one dot per poll).
4. Every step has `status == "Completed"`. For process-based steps
   (`shell`, `http`) the `exit_code` must be `0` (or `null`).
5. The set of step ids includes `init`, `fetch_weather`, `build_context`,
   `route_mood`, plus exactly one of `compose_happy` / `compose_tired` /
   `compose_neutral` (depending on the `mood` input).
6. The run log is fetched and any agent `result` line is extracted and
   printed so you can sanity-check the actual text.

On failure: non-zero exit code, the failing step is highlighted in red,
and its `error` field is dumped.

## Adding more tests

Drop a new workflow JSON into `workflows/` and run with
`--workflow workflows/your-thing.json`. The runner is intentionally
generic — it creates whatever you hand it, triggers it, and asserts that
every step completes successfully.

If/when this grows beyond a handful of workflows it would make sense to
convert `_runner.py` into a pytest harness (one test function per workflow)
or wire it into a GitHub Actions job that boots a daemon on an ephemeral
port. For now this is the seed; keep it simple.

## Cleanup

The runner DELETEs the workflow it created at the end of every run, even
on failure, unless `--keep` was passed. If you abort with Ctrl+C the
workflow id is still printed so you can clean it up manually.
