# Bruno API Collection — Agent Cron Scheduler

A [Bruno](https://www.usebruno.com/) collection covering every HTTP endpoint exposed by the ACS daemon. Open this folder in Bruno, pick the `Local` environment, and you're set.

## Layout

| Folder | Endpoints |
|---|---|
| `Health/` | `/health` (liveness, summary stats, platform service registration) |
| `Workflows/` | CRUD for workflow definitions (`/api/workflows[/:id]`), plus the trigger endpoint. |
| `Runs/` | Run records (`/api/runs/:id`), per-workflow run lists, run kill. |
| `SSE/` | Server-Sent Events stream (`/api/events/workflows`) with optional `run_id` / `workflow_id` filters. |
| `Daemon/` | Daemon process control (`/api/shutdown`, `/api/restart`, `/api/logs`). |
| `Examples/` | Ready-to-fire example requests with full payloads (1-step shell, 2-step shell+agent, etc.). |

## Variables

The `Local` environment defines:
- `baseUrl` — defaults to `http://127.0.0.1:8377` (the daemon's default bind).
- `workflowId` — set this when an endpoint takes `:id`. Most endpoints accept either a UUID or a workflow name, so you can also just type the name into the URL.
- `runId` — set after triggering a workflow if you want to chain follow-up requests.

Override `baseUrl` if you've started the daemon on a different port (`agentcronsystem start -p <port>`).

## Tips

- **Trigger then watch:** open `SSE/Stream all events.bru` first, then trigger a workflow from `Workflows/Trigger workflow.bru` — the stream will deliver `RunStarted`, `StepStarted`, `StepOutput`, `StepCompleted`, `RunCompleted` in real time.
- **Filter the stream:** `SSE/Stream events for run.bru` and `SSE/Stream events for workflow.bru` use the `run_id` / `workflow_id` query params to narrow the firehose.
- **Kill an active run:** `Runs/Kill run.bru` POSTs to `/api/runs/:run_id/kill` (no body). Returns 204.
- **Concurrency rejection:** trigger a workflow that has `allow_concurrent: false` while a run is active — you get HTTP `409 Conflict` with body `{"error":"concurrent_run_active",...}`.
- **Identifiers:** anywhere a route says `:id`, you can pass either the workflow's UUID or its `name`. The daemon resolves both.
- **Cost analytics:** workflow GET responses include a `cost_summary` block with 30-day and 1-year cost totals and run counts, populated from the daemon's in-memory cost cache.
