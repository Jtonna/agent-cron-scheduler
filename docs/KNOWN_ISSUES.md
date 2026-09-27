# Known Issues

---

## LOW — Documentation

### 1. configuration.md: `pty_rows`/`pty_cols` "No effect" wording

The values ARE read from config but are never forwarded to the spawner in
production. All three step implementations hardcode `24, 80` when invoking
the spawner (`PtySpawner::spawn` for shell/script; `PtySpawner::spawn_argv`
for agent) — see `workflow/steps/shell.rs:52`, `script.rs:65`,
`agent.rs:67` — rather than reading `config.pty_rows` / `config.pty_cols`.
A more precise description would be: "No effect with the default
NoPtySpawner; reserved for future PTY support."

---

## MEDIUM — Daemon Lifecycle

### `POST /api/shutdown` returns 200 before process exits (ACS-24)

**Symptom:** API responds 200 with `{"message": "Shutdown initiated"}` but `agentcronsystem status` or port binding may briefly indicate the process is still alive. Subsequent operations against the port (rebuilds, restarts) may fail with "access denied" until the daemon finishes its drain.

**Workaround:** Poll `GET /health` (expect connection refused) or check the PID file removal to confirm exit.

**Affects:** Observed in foreground mode (`start --foreground`). May also affect background mode — unverified.

**Tracked:** ACS-24

---

## MEDIUM — Run Persistence

### No orphan reconciliation for runs/steps left `Running` after a crash (ACS-35)

**Symptom:** If the daemon crashes or is hard-killed while a workflow run is in progress, the run's persisted record stays `status: "Running"` forever, and its in-flight step's `StepRun` row stays `status: "Running"` forever too — neither the run-level status nor the step-level row is ever reconciled on the next daemon startup.

**Why:** `finalize_run()` is the only path that writes a terminal run status, and it only runs when `run_workflow()` returns normally. A hard shutdown skips that return entirely, and there is no startup sweep that walks `workflow_runs` for stale `Running` rows and marks them `Failed`/`Killed`.

**Visibility:** This was already true at the run level before ACS-35. Since ACS-35 persists `StepRun` rows at step boundaries (mid-run, before `RunCompleted`/`RunFailed`), the same gap is now also visible at the step level: `GET /api/runs/{run_id}` will show a step stuck at `status: "Running"` with null `finished_at` indefinitely, not just the run as a whole.

**Workaround:** None automated. An operator can identify affected runs via `GET /api/runs/recent` (or `list_recent_runs`) filtered to `status: "Running"` with an implausibly old `started_at`, and correct them out-of-band.

**Tracked:** ACS-35

