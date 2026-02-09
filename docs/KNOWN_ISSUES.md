# Known Documentation Issues

Remaining issues identified during Round 3 audits (2026-02-09). All are minor and do not affect core accuracy.

---

## MEDIUM

### 1. troubleshooting.md (line 282): `-v` flag and daemon.log

The doc says `acs start --foreground -v` "produces verbose log lines to both stderr and `daemon.log`". In reality, `-v` installs a stderr-only tracing subscriber in `main.rs:11-13`. When `start_daemon()` later tries to install its dual-layer subscriber (stderr + daemon.log), `try_init()` silently fails because a subscriber is already registered. With `-v`, daemon.log will be empty/truncated.

### 2. cli-reference.md (line 240): `acs add` on non-Windows

The doc says "On macOS and Linux, the command is accepted by the CLI parser but execution returns an error." The `Commands::Add` enum variant is defined on all platforms (no `#[cfg]` gate), but the dispatch match arm is gated behind `#[cfg(target_os = "windows")]`. Needs verification of whether Rust's exhaustive match checking causes a compile error or if there's a wildcard catch-all.

---

## LOW

### 3. configuration.md: `pty_rows`/`pty_cols` "No effect" wording

The values ARE read from config and passed through the executor to the `PtySpawner` trait's `spawn()` method. They have no effect only because the default `NoPtySpawner` ignores them. A more precise description would be: "No effect with the default NoPtySpawner; reserved for future PTY support."

### 4. cli-reference.md: `--script` clap help text mismatch

The doc correctly says paths are passed verbatim, but the actual `--help` text in `cli/mod.rs:84` still says "relative to data_dir/scripts/". This is a code-side bug (stale help text), not a doc bug.

### 5. troubleshooting.md (lines 295-302): Health endpoint vs local service check

The doc says `acs status` "contacts the daemon's `/health` endpoint" and lists "Service registration status" as a displayed item. While both are true, the service status does NOT come from the `/health` endpoint -- it's computed locally by `service::is_service_registered()` in `cli/daemon.rs:303-308`.

### 6. troubleshooting.md (lines 304-308): Raw JSON and service registration

The raw JSON from `acs -v status` is the health endpoint response, which does not include service registration info, even though the formatted (non-verbose) output does.

### 7. architecture.md: `build_command()` attribution

The Windows `raw_arg()` behavior is described in the context of `Executor::build_command()` but actually lives in `NoPtySpawner::spawn()` (`pty/mod.rs:66`). The doc does correctly attribute it to `NoPtySpawner` in Section 5.3, so this is a clarity concern rather than a factual error.
