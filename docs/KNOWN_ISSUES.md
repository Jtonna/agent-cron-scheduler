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

