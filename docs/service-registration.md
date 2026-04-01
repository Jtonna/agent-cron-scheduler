# Platform-Specific Service Registration

## Overview

ACS registers itself as a **user-level service** (not system-wide) so the daemon automatically starts at login. On macOS and Linux, this does not require root or administrator privileges. On Windows, registration attempts to use the highest available privilege level (`/RL HIGHEST`), but gracefully degrades to normal privilege level if elevation is unavailable — the task is still registered and will run at login.

Each platform uses its native service manager:

| Platform | Service Manager      | Service Name                       |
|----------|----------------------|------------------------------------|
| Windows  | Task Scheduler       | `AgentCronScheduler`               |
| macOS    | launchd              | `com.agentcronsystem.scheduler`    |
| Linux    | systemd (user units) | `agentcronsystem`                  |

The cross-platform API is exposed through `acs/src/daemon/service.rs`, which delegates to a platform-specific `mod platform` block selected at compile time via `#[cfg(target_os = "...")]`.

---

## Windows

### Service Manager

Windows uses **Task Scheduler** (`schtasks.exe`). The task is created for the current user and runs at logon.

- **Task name:** `AgentCronScheduler`
- **Trigger:** `ONLOGON`
- **Run level:** `HIGHEST` (attempted first; falls back to normal privilege if elevation is unavailable)

### Install (Register)

Registration uses a two-attempt strategy:

1. **First attempt** — tries with `/RL HIGHEST` (elevated run level):

```
schtasks /Create /TN AgentCronScheduler /TR "\"<exe_path>\" start" /SC ONLOGON /RL HIGHEST /F
```

2. **Fallback attempt** — if the first attempt fails (e.g., the user is not running elevated), retries without `/RL HIGHEST`:

```
schtasks /Create /TN AgentCronScheduler /TR "\"<exe_path>\" start" /SC ONLOGON /F
```

The `/F` flag forces creation, overwriting any existing task with the same name. The quotes wrap only the executable path (to handle paths with spaces), not the entire command. The task runs `agentcronsystem start` (not `--foreground`), which means the task itself completes quickly: it spawns the daemon as a hidden background process and exits. The daemon then runs independently.

**Note:** On every background `start` invocation, the binary is automatically added to the user's PATH if not already present (Windows: User Environment Variable; Unix: shell profile). This PATH registration happens independently of service registration — it occurs regardless of whether `install_service()` succeeds or whether the service was already registered. This allows `agentcronsystem` to be invoked from any directory without specifying the full path.

### Detect Registration

```
schtasks /Query /TN AgentCronScheduler
```

A successful exit code means the task exists; a non-zero exit code means it does not.

### Start

> **Note:** On Windows, `agentcronsystem start` in background mode does **not** use `schtasks /Run`. Instead, it spawns `agentcronsystem start --foreground` directly as a hidden process via `Command::new()`. The `start_service()` function (which wraps `schtasks /Run`) is compiled on Windows but is unreachable: `cmd_start` calls `start_service()` only inside a `#[cfg(not(target_os = "windows"))]` block (`daemon.rs`), so no CLI code path invokes it on Windows.

### Stop (Service Fallback)

In practice, `agentcronsystem stop` first attempts a graceful shutdown via the HTTP API (`POST /api/shutdown`). The `schtasks /End` command is only used as a fallback when the API is unreachable and the service is registered:

```
schtasks /End /TN AgentCronScheduler
```

Terminates the running task instance.

Additionally, `agentcronsystem stop --force` bypasses both mechanisms and reads the PID file to force-kill the daemon via `taskkill /F /PID <pid>`.

### Uninstall (Unregister)

```
schtasks /Delete /TN AgentCronScheduler /F
```

---

## macOS

### Service Manager

macOS uses **launchd** with a user-level Launch Agent plist file.

- **Service name (label):** `com.agentcronsystem.scheduler`
- **Plist location:** `~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist`

### Plist Content

When `install_service` is called, the following plist file is written:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.agentcronsystem.scheduler</string>
    <key>ProgramArguments</key>
    <array>
        <string>/path/to/agentcronsystem</string>
        <string>start</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
```

Key properties:
- **`RunAtLoad`**: The service starts automatically when the plist is loaded (i.e., at user login).
- **`KeepAlive`**: launchd will restart the process if it exits, providing automatic crash recovery.
- **`--foreground`**: Under launchd, the daemon runs in foreground mode directly (launchd manages the lifecycle).

### Detect Registration

Registration is detected by checking whether the plist file exists on disk:

```rust
fn is_service_registered() -> bool {
    plist_path().exists()
}
```

### Install (Register)

1. Create the `~/Library/LaunchAgents/` directory if it does not exist.
2. Write the plist file to `~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist`.
3. Load the plist:

```
launchctl load ~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist
```

Note: If `launchctl load` fails, the error is silently ignored. Verify registration manually with `launchctl list | grep com.agentcronsystem.scheduler`.

### Start

```
launchctl start com.agentcronsystem.scheduler
```

### Stop

```
launchctl stop com.agentcronsystem.scheduler
```

### Uninstall (Unregister)

If the plist file does not exist on disk, `uninstall_service()` returns `Ok(())` immediately without performing any action. Otherwise:

1. Unload the plist:

```
launchctl unload ~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist
```

Note: If `launchctl unload` fails, the error is silently ignored. The plist file is still deleted in step 2.

2. Delete the plist file from disk.

---

## Linux

### Service Manager

Linux uses **systemd user units**.

- **Service name:** `agentcronsystem` (unit file: `agentcronsystem.service`)
- **Unit file location:** `~/.config/systemd/user/agentcronsystem.service`

### Unit File Content

When `install_service` is called, the following unit file is written:

```ini
[Unit]
Description=Agent Cron Scheduler
After=network.target

[Service]
Type=simple
ExecStart=/path/to/agentcronsystem start --foreground
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Key properties:
- **`Type=simple`**: systemd considers the service started as soon as the process is spawned.
- **`ExecStart`**: Runs `agentcronsystem start --foreground` so systemd directly manages the daemon process.
- **`Restart=on-failure`**: systemd will restart the daemon if it exits with a non-zero status.
- **`RestartSec=5`**: Wait 5 seconds before restarting after a failure.
- **`WantedBy=default.target`**: The service is enabled for the user's default login target.

### Detect Registration

Registration is detected by checking whether the unit file exists on disk:

```rust
fn is_service_registered() -> bool {
    unit_path().exists()
}
```

### Install (Register)

1. Create the `~/.config/systemd/user/` directory if it does not exist.
2. Write the unit file to `~/.config/systemd/user/agentcronsystem.service`.
3. Reload systemd, enable the service, and enable linger:

```
systemctl --user daemon-reload
systemctl --user enable agentcronsystem.service
loginctl enable-linger
```

The `loginctl enable-linger` command allows the user's systemd services to continue running after the user logs out. Without it, systemd would stop all user units when the session ends.

Note: Failures from `systemctl` and `loginctl` commands during install are silently ignored. If service registration fails, verify manually with `systemctl --user status agentcronsystem.service`.

### Start

```
systemctl --user start agentcronsystem.service
```

### Stop

```
systemctl --user stop agentcronsystem.service
```

### Uninstall (Unregister)

If the unit file does not exist on disk, `uninstall_service()` returns `Ok(())` immediately without performing any action. Otherwise:

1. Stop and disable the service:

```
systemctl --user stop agentcronsystem.service
systemctl --user disable agentcronsystem.service
```

2. Delete the unit file from disk.

3. Reload systemd:

```
systemctl --user daemon-reload
```

Note: Failures from `systemctl --user stop`, `systemctl --user disable`, and `systemctl --user daemon-reload` during uninstall are silently ignored. If the unit file cannot be deleted, the error is propagated.

For details on how `agentcronsystem start`, `agentcronsystem stop`, and `agentcronsystem uninstall` use these service registration functions, see [CLI Reference](cli-reference.md).
