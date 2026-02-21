# Known Documentation Issues

Remaining issues identified during audits. All are minor (LOW severity) and do not affect core accuracy.

---

## LOW

### 1. configuration.md: `pty_rows`/`pty_cols` "No effect" wording

The values ARE read from config and passed through the executor to the `PtySpawner` trait's `spawn()` method. They have no effect only because the default `NoPtySpawner` ignores them. A more precise description would be: "No effect with the default NoPtySpawner; reserved for future PTY support."

### 2. troubleshooting.md (lines 295-302): Health endpoint vs local service check

The doc says `acs status` "contacts the daemon's `/health` endpoint" and lists "Service registration status" as a displayed item. While both are true, the service status does NOT come from the `/health` endpoint -- it's computed locally by `service::is_service_registered()` in `cli/daemon.rs:303-308`.

### 3. troubleshooting.md (lines 304-308): Raw JSON and service registration

The raw JSON from `acs -v status` is the health endpoint response, which does not include service registration info, even though the formatted (non-verbose) output does.

### 4. architecture.md: `build_command()` attribution

The Windows `raw_arg()` behavior is described in the context of `Executor::build_command()` but actually lives in `NoPtySpawner::spawn()` (`pty/mod.rs:72`). The doc does correctly attribute it to `NoPtySpawner` in Section 5.3, so this is a clarity concern rather than a factual error.
