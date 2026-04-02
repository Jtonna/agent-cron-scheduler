# Platform-Specific Service Registration

## Overview

ACS registers itself as a **user-level service** (not system-wide) so the daemon automatically starts at login. On all supported platforms this does not require root or administrator privileges — Windows uses a HKCU registry write, which requires no elevation.

Each platform uses its native service manager:

| Platform | Service Manager        | Service Name                       |
|----------|------------------------|------------------------------------|
| Windows  | Registry Run key       | `AgentCronScheduler`               |
| macOS    | launchd                | `com.agentcronsystem.scheduler`    |
| Linux    | systemd (user units)   | `agentcronsystem`                  |

The cross-platform API is exposed through `acs/src/daemon/service.rs`, which delegates to a platform-specific `mod platform` block selected at compile time via `#[cfg(target_os = "...")]`.

---

## Windows

### Service Manager

Windows uses the **Registry Run key** (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`). A `REG_SZ` value is created under that key so the daemon launches automatically at user logon. No elevation is required — writes to `HKCU` are always permitted for the current user.

- **Value name:** `AgentCronScheduler`
- **Value data:** `"<exe_path>" start`
- **Trigger:** user logon (Windows reads all `Run` key values at logon)

### Install (Register)

Registration is a single registry write:

```
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
  AgentCronScheduler = "<exe_path>" start
```

The value data quotes only the executable path (to handle paths with spaces) and appends the `start` sub-command. Writing the value when one already exists silently overwrites it, so the operation is idempotent.

**Note:** On every background `start` invocation, the binary is automatically added to the user's PATH if not already present (Windows: User Environment Variable; Unix: shell profile). This PATH registration happens independently of service registration — it occurs regardless of whether `install_service()` succeeds or whether the service was already registered. This allows `agentcronsystem` to be invoked from any directory without specifying the full path.

### Detect Registration

Registration is detected by checking whether the `AgentCronScheduler` value is present under the Run key:

```rust
fn is_service_registered() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey_with_flags(RUN_KEY_PATH, KEY_READ) {
        Ok(key) => key.get_value::<String, _>(RUN_VALUE_NAME).is_ok(),
        Err(_) => false,
    }
}
```

### Start

> **Note:** `start_service()` is not applicable for registry-based auto-start. On Windows, `acs start` spawns the daemon directly as a hidden background process via `Command::new()`. The Run key entry only causes Windows to launch the daemon automatically at the next logon — it is not used to start the daemon on demand.

### Stop

> **Note:** `stop_service()` is not applicable for registry-based auto-start. `acs stop` first attempts a graceful shutdown via the HTTP API (`POST /api/shutdown`). If the API is unreachable, it falls back to a PID-based kill via `taskkill /F /PID <pid>`.

### Uninstall (Unregister)

Uninstallation deletes the `AgentCronScheduler` value from the Run key. The operation is idempotent — if the value is already absent, `uninstall_service()` returns `Ok(())` without error.

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
