# Known Issues

---

## LOW — Documentation

### 1. configuration.md: `pty_rows`/`pty_cols` "No effect" wording

The values ARE read from config but are never forwarded to the spawner in
production. All three step implementations that call `PtySpawner::spawn()`
hardcode `24, 80` (`workflow/steps/shell.rs:48`, `script.rs:63`,
`agent.rs:121`) rather than reading `config.pty_rows` / `config.pty_cols`.
A more precise description would be: "No effect with the default
NoPtySpawner; reserved for future PTY support."

### 2. troubleshooting.md (lines 295-302): Health endpoint vs local service check

The doc says `acs status` "contacts the daemon's `/health` endpoint" and
lists "Service registration status" as a displayed item. While both are
true, the service status does NOT come from the `/health` endpoint — it is
computed locally by `service::is_service_registered()` in
`cli/daemon.rs:505`.

### 3. troubleshooting.md (lines 304-308): Raw JSON and service registration

The raw JSON from `acs -v status` is the health endpoint response, which
does not include service registration info, even though the formatted
(non-verbose) output does (`cli/daemon.rs:542`, `cli/daemon.rs:544-547`).

---

## LOW — Runtime behavior

### 4. Cron-fired runs cannot be killed via `POST /api/runs/{id}/kill`

The scheduler dispatches runs with `kill_signals: None`
(`daemon/scheduler.rs:281`), so the kill-signal registry entry is never
inserted for cron-fired runs. `POST /api/runs/{run_id}/kill` will return
a 404 for those runs because the registry lookup at
`server/workflow_routes.rs:492` finds no sender. Manually triggered runs
(via `POST /api/workflows/{id}/trigger`) do wire kill signals and are
killable.
