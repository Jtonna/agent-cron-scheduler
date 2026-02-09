# Platform-Specific Service Registration

## Overview

ACS registers itself as a **user-level service** (not system-wide) so the daemon automatically starts at login without requiring root or administrator privileges. The service runs under the current user's session, inheriting their full environment (PATH, home directory, etc.).

Each platform uses its native service manager:

| Platform | Service Manager      | Service Name           |
|----------|----------------------|------------------------|
| Windows  | Task Scheduler       | `AgentCronScheduler`   |
| macOS    | launchd              | `com.acs.scheduler`    |
| Linux    | systemd (user units) | `acs`                  |

The cross-platform API is exposed through `acs/src/daemon/service.rs`, which delegates to a platform-specific `mod platform` block selected at compile time via `#[cfg(target_os = "...")]`.

---

## Windows

### Service Manager

Windows uses **Task Scheduler** (`schtasks.exe`). The task is created for the current user and runs at logon.

- **Task name:** `AgentCronScheduler`
- **Trigger:** `ONLOGON`
- **Run level:** `HIGHEST`

### Install (Register)

```
schtasks /Create /TN AgentCronScheduler /TR "<exe_path> start" /SC ONLOGON /RL HIGHEST /F
```

The `/F` flag forces creation, overwriting any existing task with the same name. The task runs `acs start` (not `--foreground`), which means the task itself completes quickly: it spawns the daemon as a hidden background process and exits. The daemon then runs independently.

### Detect Registration

```
schtasks /Query /TN AgentCronScheduler
```

A successful exit code means the task exists; a non-zero exit code means it does not.

### Start

> **Note:** On Windows, `acs start` in background mode does **not** use `schtasks /Run`. Instead, it spawns `acs start --foreground` directly as a hidden process via `Command::new()`. The `schtasks /Run` function exists in the codebase but is not called during normal operation.

### Stop

```
schtasks /End /TN AgentCronScheduler
```

Hard-kills the running task instance.

### Uninstall (Unregister)

```
schtasks /Delete /TN AgentCronScheduler /F
```

---

## macOS

### Service Manager

macOS uses **launchd** with a user-level Launch Agent plist file.

- **Service name (label):** `com.acs.scheduler`
- **Plist location:** `~/Library/LaunchAgents/com.acs.scheduler.plist`

### Plist Content

When `install_service` is called, the following plist file is written:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.acs.scheduler</string>
    <key>ProgramArguments</key>
    <array>
        <string>/path/to/acs</string>
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
2. Write the plist file to `~/Library/LaunchAgents/com.acs.scheduler.plist`.
3. Load the plist:

```
launchctl load ~/Library/LaunchAgents/com.acs.scheduler.plist
```

### Start

```
launchctl start com.acs.scheduler
```

### Stop

```
launchctl stop com.acs.scheduler
```

### Uninstall (Unregister)

1. Unload the plist:

```
launchctl unload ~/Library/LaunchAgents/com.acs.scheduler.plist
```

2. Delete the plist file from disk.

---

## Linux

### Service Manager

Linux uses **systemd user units**.

- **Service name:** `acs` (unit file: `acs.service`)
- **Unit file location:** `~/.config/systemd/user/acs.service`

### Unit File Content

When `install_service` is called, the following unit file is written:

```ini
[Unit]
Description=Agent Cron Scheduler
After=network.target

[Service]
Type=simple
ExecStart=/path/to/acs start --foreground
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
```

Key properties:
- **`Type=simple`**: systemd considers the service started as soon as the process is spawned.
- **`ExecStart`**: Runs `acs start --foreground` so systemd directly manages the daemon process.
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
2. Write the unit file to `~/.config/systemd/user/acs.service`.
3. Reload systemd, enable the service, and enable linger:

```
systemctl --user daemon-reload
systemctl --user enable acs.service
loginctl enable-linger
```

The `loginctl enable-linger` command allows the user's systemd services to continue running after the user logs out. Without it, systemd would stop all user units when the session ends.

### Start

```
systemctl --user start acs.service
```

### Stop

```
systemctl --user stop acs.service
```

### Uninstall (Unregister)

1. Stop and disable the service:

```
systemctl --user stop acs.service
systemctl --user disable acs.service
```

2. Delete the unit file from disk.

3. Reload systemd:

```
systemctl --user daemon-reload
```

For details on how `acs start`, `acs stop`, and `acs uninstall` use these service registration functions, see [CLI Reference](cli-reference.md).
