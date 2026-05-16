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

