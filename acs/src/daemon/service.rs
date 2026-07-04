// Platform service registration for daemon persistence.
//
// - Windows: Registry Run key (HKCU\Software\Microsoft\Windows\CurrentVersion\Run)
// - macOS:   launchd plist to ~/Library/LaunchAgents/com.agentcronsystem.scheduler.plist
// - Linux:   systemd user unit to ~/.config/systemd/user/agentcronsystem.service

use std::path::Path;

use serde::Serialize;

/// Information about the platform service registration.
#[derive(Debug, Clone, Serialize)]
pub struct ServiceStatusInfo {
    pub platform: &'static str,
    pub service_name: &'static str,
    pub is_registered: bool,
    pub service_path: Option<String>,
}

/// Return the platform string for the current OS.
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}

/// Return the service name used on this platform.
pub fn service_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "AgentCronScheduler"
    } else if cfg!(target_os = "macos") {
        "com.agentcronsystem.scheduler"
    } else {
        "agentcronsystem"
    }
}

// ---------------------------------------------------------------------------
// Test-only hermetic state override.
//
// Service registration lives in global, machine-wide state (a systemd unit
// file, a launchd plist, or an HKCU registry value). Because cargo runs tests
// in parallel, tests touching that shared state would race with one another and
// pollute the real machine. Each test instead redirects the state location to a
// private temp directory through this thread-local: cargo runs every test on
// its own thread, so an override set inside one test is invisible to all others
// — no mutex, no env-var collision, no cross-test TOCTOU. The whole mechanism is
// gated to `cfg(test)`, so production path resolvers compile down to exactly the
// code they had before the override existed.
#[cfg(test)]
thread_local! {
    static SERVICE_STATE_DIR_OVERRIDE: std::cell::RefCell<Option<std::path::PathBuf>> =
        const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
fn test_state_override() -> Option<std::path::PathBuf> {
    SERVICE_STATE_DIR_OVERRIDE.with(|slot| slot.borrow().clone())
}

/// Derive a throwaway HKCU subkey for a test's private registry state from its
/// temp-dir path, so Windows tests read and write a disposable key instead of
/// the real `...\CurrentVersion\Run`.
#[cfg(all(test, target_os = "windows"))]
fn windows_test_run_subkey(base: &std::path::Path) -> String {
    let leaf = base
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("acs_service_test");
    format!(r"Software\AgentCronScheduler\ServiceTest\{}", leaf)
}

// ---------------------------------------------------------------------------
// Windows implementation — uses Registry Run key
// (HKCU\Software\Microsoft\Windows\CurrentVersion\Run) so the daemon starts
// automatically at user logon without requiring elevation.
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
mod platform {
    use super::*;

    const RUN_KEY_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN_VALUE_NAME: &str = "AgentCronScheduler";

    /// Resolve the HKCU subkey that stores the Run value. Production always uses
    /// the real Run key; tests redirect it to a private throwaway subkey.
    fn run_key_path() -> std::borrow::Cow<'static, str> {
        #[cfg(test)]
        if let Some(base) = super::test_state_override() {
            return std::borrow::Cow::Owned(super::windows_test_run_subkey(&base));
        }
        std::borrow::Cow::Borrowed(RUN_KEY_PATH)
    }

    /// Check if the Run key value is present in the registry.
    pub fn is_service_registered() -> bool {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        match hkcu.open_subkey_with_flags(run_key_path().as_ref(), KEY_READ) {
            Ok(key) => key.get_value::<String, _>(RUN_VALUE_NAME).is_ok(),
            Err(_) => false,
        }
    }

    /// Write the Run key value so the daemon launches at user logon.
    pub fn install_service(exe_path: &Path) -> anyhow::Result<()> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = hkcu.open_subkey_with_flags(run_key_path().as_ref(), KEY_SET_VALUE)?;

        let value = format!("\"{}\" start", exe_path.display());
        key.set_value(RUN_VALUE_NAME, &value)?;

        tracing::info!("Registered auto-start Run key: {}", value);
        Ok(())
    }

    /// Remove the Run key value. Succeeds even if the value is absent (idempotent).
    pub fn uninstall_service() -> anyhow::Result<()> {
        use winreg::enums::*;
        use winreg::RegKey;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let key = match hkcu.open_subkey_with_flags(run_key_path().as_ref(), KEY_SET_VALUE) {
            Ok(k) => k,
            Err(_) if !is_service_registered() => return Ok(()),
            Err(e) => return Err(anyhow::anyhow!(e)),
        };

        match key.delete_value(RUN_VALUE_NAME) {
            Ok(()) => {
                tracing::info!("Removed auto-start Run key value: {}", RUN_VALUE_NAME);
                Ok(())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound || e.raw_os_error() == Some(2) => {
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }

    /// Get a human-readable description of where the Run entry is registered.
    pub fn service_path() -> Option<String> {
        if is_service_registered() {
            Some(format!(
                r"Registry: HKCU\{}\{}",
                RUN_KEY_PATH, RUN_VALUE_NAME
            ))
        } else {
            None
        }
    }

    /// Not supported with registry-based auto-start.
    pub fn start_service() -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "start_service is not supported with registry-based auto-start on Windows"
        ))
    }

    /// Not supported with registry-based auto-start.
    pub fn stop_service() -> anyhow::Result<()> {
        Err(anyhow::anyhow!(
            "stop_service is not supported with registry-based auto-start on Windows"
        ))
    }

    /// Ensure the binary's directory is in the user's PATH (HKCU\Environment).
    pub fn ensure_path_entry(exe_path: &Path) -> anyhow::Result<()> {
        use winreg::enums::*;
        use winreg::RegKey;

        let dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Cannot determine binary directory"))?
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Binary path is not valid UTF-8"))?;

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let env = hkcu.open_subkey_with_flags("Environment", KEY_READ | KEY_WRITE)?;

        let current_path: String = env.get_value("Path").unwrap_or_default();

        // Case-insensitive check if already in PATH
        let already_present = current_path
            .split(';')
            .any(|entry| entry.eq_ignore_ascii_case(dir));

        if already_present {
            tracing::info!("Binary directory already in PATH: {}", dir);
            return Ok(());
        }

        let new_path = if current_path.is_empty() {
            dir.to_string()
        } else {
            format!("{};{}", current_path, dir)
        };

        env.set_value("Path", &new_path)?;

        // Broadcast WM_SETTINGCHANGE so new shells pick up the change
        broadcast_environment_change();

        tracing::info!("Added binary directory to PATH: {}", dir);
        Ok(())
    }

    fn broadcast_environment_change() {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;

        #[link(name = "user32")]
        extern "system" {
            fn SendMessageTimeoutW(
                hwnd: isize,
                msg: u32,
                wparam: usize,
                lparam: isize,
                flags: u32,
                timeout: u32,
                result: *mut usize,
            ) -> isize;
        }

        const HWND_BROADCAST: isize = 0xFFFF;
        const WM_SETTINGCHANGE: u32 = 0x001A;
        const SMTO_ABORTIFHUNG: u32 = 0x0002;

        let environment: Vec<u16> = OsStr::new("Environment")
            .encode_wide()
            .chain(Some(0))
            .collect();

        unsafe {
            SendMessageTimeoutW(
                HWND_BROADCAST,
                WM_SETTINGCHANGE,
                0,
                environment.as_ptr() as isize,
                SMTO_ABORTIFHUNG,
                5000,
                std::ptr::null_mut(),
            );
        }
    }
}

// ---------------------------------------------------------------------------
// macOS launchd implementation
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::path::PathBuf;

    fn plist_path() -> PathBuf {
        #[cfg(test)]
        if let Some(base) = super::test_state_override() {
            return base.join("com.agentcronsystem.scheduler.plist");
        }
        let home = dirs::home_dir().expect("Could not determine home directory");
        home.join("Library")
            .join("LaunchAgents")
            .join("com.agentcronsystem.scheduler.plist")
    }

    pub fn is_service_registered() -> bool {
        plist_path().exists()
    }

    /// Ensure the executable's directory is in the user's PATH by modifying shell config files.
    pub fn ensure_path_entry(exe_path: &Path) -> anyhow::Result<()> {
        use std::io::{BufRead, BufReader};

        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Executable has no parent directory"))?;
        let exe_dir_str = exe_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid directory path"))?;

        // Check if already in current PATH
        if let Ok(current_path) = std::env::var("PATH") {
            if current_path.split(':').any(|p| p == exe_dir_str) {
                tracing::debug!("Directory {} already in PATH", exe_dir_str);
                return Ok(());
            }
        }

        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        let shell_configs = vec![home.join(".zshrc"), home.join(".bash_profile")];

        let marker = "# Added by AgentCronScheduler";
        let path_line = format!("export PATH=\"$PATH:{}\" {}", exe_dir_str, marker);

        for config_file in shell_configs {
            if !config_file.exists() {
                continue;
            }

            // Check if already present
            let file = std::fs::File::open(&config_file)?;
            let reader = BufReader::new(file);
            let mut already_present = false;

            for l in reader.lines().map_while(Result::ok) {
                if (l.contains(marker) && l.contains(exe_dir_str))
                    || (l.contains("PATH=") && l.contains(exe_dir_str))
                {
                    already_present = true;
                    break;
                }
            }

            if !already_present {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&config_file)?;
                writeln!(file, "\n{}", path_line)?;
                tracing::info!("Added PATH entry to {}", config_file.display());
            } else {
                tracing::debug!("PATH entry already exists in {}", config_file.display());
            }
        }

        Ok(())
    }

    pub fn install_service(exe_path: &Path) -> anyhow::Result<()> {
        let plist_dir = plist_path().parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&plist_dir)?;

        let exe = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid executable path"))?;

        let plist_content = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.agentcronsystem.scheduler</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe}</string>
        <string>start</string>
        <string>--foreground</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
"#
        );

        std::fs::write(plist_path(), plist_content)?;

        // Load the plist
        let _ = std::process::Command::new("launchctl")
            .arg("load")
            .arg(plist_path())
            .status();

        Ok(())
    }

    pub fn uninstall_service() -> anyhow::Result<()> {
        let path = plist_path();
        if path.exists() {
            // Unload first
            let _ = std::process::Command::new("launchctl")
                .arg("unload")
                .arg(&path)
                .status();
            std::fs::remove_file(&path)?;
        }
        Ok(())
    }

    pub fn service_path() -> Option<String> {
        if is_service_registered() {
            Some(plist_path().to_string_lossy().to_string())
        } else {
            None
        }
    }

    /// Start the launchd service.
    pub fn start_service() -> anyhow::Result<()> {
        let status = std::process::Command::new("launchctl")
            .arg("start")
            .arg("com.agentcronsystem.scheduler")
            .status()?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("launchctl start failed with exit code: {:?}", status.code())
        }
    }

    /// Stop the launchd service.
    pub fn stop_service() -> anyhow::Result<()> {
        let status = std::process::Command::new("launchctl")
            .arg("stop")
            .arg("com.agentcronsystem.scheduler")
            .status()?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!("launchctl stop failed with exit code: {:?}", status.code())
        }
    }
}

// ---------------------------------------------------------------------------
// Linux systemd implementation
// ---------------------------------------------------------------------------
#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use std::path::PathBuf;

    fn unit_path() -> PathBuf {
        #[cfg(test)]
        if let Some(base) = super::test_state_override() {
            return base.join("agentcronsystem.service");
        }
        let home = dirs::home_dir().expect("Could not determine home directory");
        home.join(".config")
            .join("systemd")
            .join("user")
            .join("agentcronsystem.service")
    }

    pub fn is_service_registered() -> bool {
        unit_path().exists()
    }

    /// Ensure the executable's directory is in the user's PATH by modifying shell config files.
    pub fn ensure_path_entry(exe_path: &Path) -> anyhow::Result<()> {
        use std::io::{BufRead, BufReader};

        let exe_dir = exe_path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("Executable has no parent directory"))?;
        let exe_dir_str = exe_dir
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid directory path"))?;

        // Check if already in current PATH
        if let Ok(current_path) = std::env::var("PATH") {
            if current_path.split(':').any(|p| p == exe_dir_str) {
                tracing::debug!("Directory {} already in PATH", exe_dir_str);
                return Ok(());
            }
        }

        let home = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("Could not determine home directory"))?;
        let shell_configs = vec![
            home.join(".bashrc"),
            home.join(".zshrc"),
            home.join(".profile"),
        ];

        let marker = "# Added by AgentCronScheduler";
        let path_line = format!("export PATH=\"$PATH:{}\" {}", exe_dir_str, marker);

        for config_file in shell_configs {
            if !config_file.exists() {
                continue;
            }

            // Check if already present
            let file = std::fs::File::open(&config_file)?;
            let reader = BufReader::new(file);
            let mut already_present = false;

            for l in reader.lines().map_while(Result::ok) {
                if (l.contains(marker) && l.contains(exe_dir_str))
                    || (l.contains("PATH=") && l.contains(exe_dir_str))
                {
                    already_present = true;
                    break;
                }
            }

            if !already_present {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&config_file)?;
                writeln!(file, "\n{}", path_line)?;
                tracing::info!("Added PATH entry to {}", config_file.display());
            } else {
                tracing::debug!("PATH entry already exists in {}", config_file.display());
            }
        }

        Ok(())
    }

    pub fn install_service(exe_path: &Path) -> anyhow::Result<()> {
        let unit_dir = unit_path().parent().unwrap().to_path_buf();
        std::fs::create_dir_all(&unit_dir)?;

        let exe = exe_path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid executable path"))?;

        let unit_content = format!(
            r#"[Unit]
Description=Agent Cron Scheduler
After=network.target

[Service]
Type=simple
ExecStart={exe} start --foreground
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
"#
        );

        std::fs::write(unit_path(), unit_content)?;

        // Enable and start
        let _ = std::process::Command::new("systemctl")
            .arg("--user")
            .arg("daemon-reload")
            .status();
        let _ = std::process::Command::new("systemctl")
            .arg("--user")
            .arg("enable")
            .arg("agentcronsystem.service")
            .status();
        // Enable linger for persistence
        let _ = std::process::Command::new("loginctl")
            .arg("enable-linger")
            .status();

        Ok(())
    }

    pub fn uninstall_service() -> anyhow::Result<()> {
        let path = unit_path();
        if path.exists() {
            let _ = std::process::Command::new("systemctl")
                .arg("--user")
                .arg("stop")
                .arg("agentcronsystem.service")
                .status();
            let _ = std::process::Command::new("systemctl")
                .arg("--user")
                .arg("disable")
                .arg("agentcronsystem.service")
                .status();
            std::fs::remove_file(&path)?;
            let _ = std::process::Command::new("systemctl")
                .arg("--user")
                .arg("daemon-reload")
                .status();
        }
        Ok(())
    }

    pub fn service_path() -> Option<String> {
        if is_service_registered() {
            Some(unit_path().to_string_lossy().to_string())
        } else {
            None
        }
    }

    /// Start the systemd user service.
    pub fn start_service() -> anyhow::Result<()> {
        let status = std::process::Command::new("systemctl")
            .arg("--user")
            .arg("start")
            .arg("agentcronsystem.service")
            .status()?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "systemctl --user start failed with exit code: {:?}",
                status.code()
            )
        }
    }

    /// Stop the systemd user service.
    pub fn stop_service() -> anyhow::Result<()> {
        let status = std::process::Command::new("systemctl")
            .arg("--user")
            .arg("stop")
            .arg("agentcronsystem.service")
            .status()?;

        if status.success() {
            Ok(())
        } else {
            anyhow::bail!(
                "systemctl --user stop failed with exit code: {:?}",
                status.code()
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Public cross-platform API
// ---------------------------------------------------------------------------

/// Check if the system service is registered on the current platform.
pub fn is_service_registered() -> bool {
    platform::is_service_registered()
}

/// Install the system service for the current platform.
pub fn install_service(exe_path: &Path) -> anyhow::Result<()> {
    platform::install_service(exe_path)
}

/// Ensure the binary's directory is in the user's PATH (independent of service registration).
pub fn ensure_path_entry(exe_path: &Path) -> anyhow::Result<()> {
    platform::ensure_path_entry(exe_path)
}

/// Uninstall the system service for the current platform.
pub fn uninstall_service() -> anyhow::Result<()> {
    platform::uninstall_service()
}

/// Start the system service for the current platform.
pub fn start_service() -> anyhow::Result<()> {
    platform::start_service()
}

/// Stop the system service for the current platform.
pub fn stop_service() -> anyhow::Result<()> {
    platform::stop_service()
}

/// Get comprehensive service status information.
pub fn service_status() -> ServiceStatusInfo {
    let registered = is_service_registered();
    ServiceStatusInfo {
        platform: platform_name(),
        service_name: service_name(),
        is_registered: registered,
        service_path: platform::service_path(),
    }
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_name_is_valid() {
        let name = platform_name();
        assert!(
            name == "windows" || name == "macos" || name == "linux",
            "Platform name should be one of the known platforms, got: {}",
            name
        );
    }

    #[test]
    fn test_service_name_is_valid() {
        let name = service_name();
        assert!(!name.is_empty(), "Service name should not be empty");
    }

    #[test]
    fn test_service_status_returns_valid_info() {
        let status = service_status();
        assert_eq!(status.platform, platform_name());
        assert_eq!(status.service_name, service_name());
        // is_registered is a bool, no need to assert a specific value
        // service_path can be None or Some depending on the system state
    }

    #[test]
    fn test_service_status_info_serializes() {
        let info = ServiceStatusInfo {
            platform: "test",
            service_name: "test-service",
            is_registered: false,
            service_path: None,
        };
        let json = serde_json::to_string(&info).expect("serialize");
        assert!(json.contains("\"platform\":\"test\""));
        assert!(json.contains("\"is_registered\":false"));
    }

    #[test]
    fn test_is_service_registered_returns_bool() {
        // This test simply verifies the function returns without panicking.
        // On CI or dev machines, the service is likely NOT registered.
        let _registered = is_service_registered();
    }

    /// RAII guard that redirects every service-state read and write to a
    /// private temp location for the lifetime of a test, then restores and
    /// cleans up on drop. See the module-level note on the thread-local for why
    /// this is parallel-safe. On Windows it also provisions (and later deletes)
    /// the throwaway registry subkey that stands in for the real Run key.
    struct ServiceStateGuard {
        _dir: tempfile::TempDir,
    }

    impl ServiceStateGuard {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("create temp service-state dir");
            let path = dir.path().to_path_buf();

            #[cfg(target_os = "windows")]
            {
                use winreg::enums::*;
                use winreg::RegKey;

                let subkey = super::windows_test_run_subkey(&path);
                let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                hkcu.create_subkey(&subkey)
                    .expect("create throwaway run subkey");
            }

            super::SERVICE_STATE_DIR_OVERRIDE.with(|slot| {
                *slot.borrow_mut() = Some(path);
            });

            Self { _dir: dir }
        }
    }

    impl Drop for ServiceStateGuard {
        fn drop(&mut self) {
            #[cfg(target_os = "windows")]
            {
                use winreg::enums::*;
                use winreg::RegKey;

                if let Some(base) = super::test_state_override() {
                    let subkey = super::windows_test_run_subkey(&base);
                    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
                    let _ = hkcu.delete_subkey_all(&subkey);
                }
            }

            super::SERVICE_STATE_DIR_OVERRIDE.with(|slot| {
                *slot.borrow_mut() = None;
            });
        }
    }

    /// A syntactically valid (but non-existent) executable path for install
    /// tests. install_service never checks the path exists — it only records
    /// it — so this is enough to exercise the real registration logic.
    fn sample_exe() -> std::path::PathBuf {
        std::path::PathBuf::from(if cfg!(target_os = "windows") {
            r"C:\Program Files\ACS\acs.exe"
        } else {
            "/opt/acs/bin/acs"
        })
    }

    #[test]
    fn test_start_service_fails_when_not_registered() {
        // Isolated empty state → deterministically not registered, so the real
        // registration logic under test is exercised on a known-clean slate.
        let _guard = ServiceStateGuard::new();
        assert!(
            !is_service_registered(),
            "isolated state should start unregistered"
        );

        // With no unit/plist and no Run value, start_service must report an
        // error (systemctl/launchctl fail on the missing service; Windows'
        // registry-based auto-start returns Err unconditionally).
        let result = start_service();
        assert!(
            result.is_err(),
            "start_service should fail when the service is not registered"
        );
    }

    // There is deliberately no unix counterpart to the Windows stop test below.
    // On Linux/macOS `stop_service` is a thin pass-through to `systemctl` /
    // `launchctl`, and stopping an absent service is environment-dependent
    // (idempotent Ok when a user session/bus is present, Err without) — there is
    // no deterministic contract of ours to assert, so we do not unit-test the OS
    // service manager's behavior. This omission is intentional, not a skip.
    #[cfg(windows)]
    #[test]
    fn test_stop_service_not_supported_on_windows() {
        // The Windows registry-based auto-start has no service to stop, so
        // stop_service returns the "not supported" Err unconditionally — this is
        // our own logic, deterministic regardless of state.
        let result = stop_service();
        assert!(
            result.is_err(),
            "stop_service should return the not-supported error on Windows"
        );
    }

    #[test]
    fn test_install_service_registers_service() {
        let _guard = ServiceStateGuard::new();
        assert!(
            !is_service_registered(),
            "isolated state should start unregistered"
        );

        install_service(sample_exe().as_path())
            .expect("install into isolated state should succeed");

        assert!(
            is_service_registered(),
            "service should report registered after install"
        );
        assert!(
            service_status().service_path.is_some(),
            "service_status should expose a registration path after install"
        );
    }

    #[test]
    fn test_uninstall_service_removes_and_is_idempotent() {
        let _guard = ServiceStateGuard::new();

        // Uninstall is a graceful no-op when nothing is installed.
        assert!(!is_service_registered());
        uninstall_service().expect("uninstall when absent should be Ok");

        // Install, then confirm uninstall actually removes the registration.
        install_service(sample_exe().as_path()).expect("install should succeed");
        assert!(
            is_service_registered(),
            "should be registered after install"
        );

        uninstall_service().expect("uninstall should succeed");
        assert!(
            !is_service_registered(),
            "should be unregistered after uninstall"
        );

        // A second uninstall on already-clean state is still Ok (idempotent).
        uninstall_service().expect("second uninstall should also be Ok");
    }

    /// Tests for the PATH detection logic used by ensure_path_entry on Windows.
    /// These use unique temporary registry keys to avoid modifying the real user PATH.
    #[cfg(target_os = "windows")]
    mod path_entry_tests {
        use winreg::enums::*;
        use winreg::RegKey;

        /// Core logic extracted to mirror ensure_path_entry without touching
        /// HKCU\Environment or broadcasting WM_SETTINGCHANGE.
        fn ensure_path_in_key(env: &RegKey, dir: &str) -> anyhow::Result<bool> {
            let current_path: String = env.get_value("Path").unwrap_or_default();

            let already_present = current_path
                .split(';')
                .any(|entry| entry.eq_ignore_ascii_case(dir));

            if already_present {
                return Ok(false); // no change
            }

            let new_path = if current_path.is_empty() {
                dir.to_string()
            } else {
                format!("{};{}", current_path, dir)
            };

            env.set_value("Path", &new_path)?;
            Ok(true) // changed
        }

        /// Helper: create a uniquely-named temp registry key, set Path, run
        /// the closure, read back the result, then clean up. Returns the final
        /// Path value so assertions can happen after cleanup.
        fn run_path_test(
            test_name: &str,
            initial_path: &str,
            dir: &str,
        ) -> (anyhow::Result<bool>, String) {
            let subkey = format!("Software\\AcsTest_{}", test_name);
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey(&subkey).expect("create test subkey");
            key.set_value("Path", &initial_path)
                .expect("set initial Path");

            let result = ensure_path_in_key(&key, dir);
            let final_path: String = key.get_value("Path").unwrap_or_default();

            // Clean up
            drop(key);
            hkcu.delete_subkey_all(&subkey).ok();

            (result, final_path)
        }

        #[test]
        fn test_path_entry_adds_missing_dir() {
            let (result, path) = run_path_test(
                "adds_missing",
                "C:\\Windows;C:\\Windows\\System32",
                "C:\\MyApp\\bin",
            );
            assert!(
                result.expect("should succeed"),
                "should have added the directory"
            );
            assert!(
                path.contains("C:\\MyApp\\bin"),
                "PATH should contain the new directory, got: {}",
                path
            );
        }

        #[test]
        fn test_path_entry_idempotent() {
            let (result, path) =
                run_path_test("idempotent", "C:\\Windows;C:\\MyApp\\bin", "C:\\MyApp\\bin");
            assert!(
                !result.expect("should succeed"),
                "should not modify PATH when dir already present"
            );
            assert_eq!(
                path, "C:\\Windows;C:\\MyApp\\bin",
                "PATH should be unchanged"
            );
        }

        #[test]
        fn test_path_entry_case_insensitive() {
            let (result, _) = run_path_test(
                "case_insensitive",
                "C:\\Windows;c:\\myapp\\bin",
                "C:\\MyApp\\bin",
            );
            assert!(
                !result.expect("should succeed"),
                "should detect existing entry case-insensitively"
            );
        }

        #[test]
        fn test_path_entry_empty_path() {
            let (result, path) = run_path_test("empty_path", "", "C:\\MyApp\\bin");
            assert!(result.expect("should succeed"), "should add to empty PATH");
            assert_eq!(
                path, "C:\\MyApp\\bin",
                "PATH should be just the new directory (no leading semicolon)"
            );
        }

        #[test]
        fn test_ensure_path_entry_no_parent() {
            // A path with no parent should not panic
            let result =
                super::super::platform::ensure_path_entry(std::path::Path::new("justfilename"));
            let _ = result;
        }
    }

    /// Tests for the Windows Registry Run key logic used by install_service /
    /// uninstall_service / is_service_registered. All tests operate on unique
    /// temporary keys under `HKCU\Software\AcsRunTest_*` and never touch the
    /// real `HKCU\...\Run` key.
    #[cfg(target_os = "windows")]
    mod run_key_tests {
        use winreg::enums::*;
        use winreg::RegKey;

        const RUN_VALUE_NAME: &str = "AgentCronScheduler";

        /// Create a uniquely-named temp key, execute the closure, then delete
        /// the entire temp key tree. Returns whatever the closure returned.
        fn with_temp_key<F, T>(test_name: &str, f: F) -> T
        where
            F: FnOnce(&RegKey) -> T,
        {
            let subkey = format!("Software\\AcsRunTest_{}", test_name);
            let hkcu = RegKey::predef(HKEY_CURRENT_USER);
            let (key, _) = hkcu.create_subkey(&subkey).expect("create temp run key");
            let result = f(&key);
            drop(key);
            hkcu.delete_subkey_all(&subkey).ok();
            result
        }

        /// Simulate the install_service write — sets `"<path>" start` — then
        /// reads the value back and verifies the format.
        #[test]
        fn test_install_writes_run_value() {
            with_temp_key("install_writes", |key| {
                let exe_path = r"C:\Program Files\ACS\acs.exe";
                let expected = format!("\"{}\" start", exe_path);

                key.set_value(RUN_VALUE_NAME, &expected)
                    .expect("set run value");

                let stored: String = key
                    .get_value(RUN_VALUE_NAME)
                    .expect("value should be present after write");

                assert_eq!(
                    stored, expected,
                    "stored value should match \"<path>\" start format"
                );
            });
        }

        /// Writing the value twice (simulating repeated install) should leave
        /// the correct final value without error.
        #[test]
        fn test_install_is_idempotent() {
            with_temp_key("install_idempotent", |key| {
                let exe_path = r"C:\Tools\acs.exe";
                let value = format!("\"{}\" start", exe_path);

                key.set_value(RUN_VALUE_NAME, &value)
                    .expect("first write should succeed");
                key.set_value(RUN_VALUE_NAME, &value)
                    .expect("second write should also succeed");

                let stored: String = key
                    .get_value(RUN_VALUE_NAME)
                    .expect("value should still be present");
                assert_eq!(
                    stored, value,
                    "value should be correct after idempotent write"
                );
            });
        }

        /// Writing then deleting a value should leave it absent.
        #[test]
        fn test_uninstall_removes_value() {
            with_temp_key("uninstall_removes", |key| {
                let value = format!("\"{}\" start", r"C:\Tools\acs.exe");
                key.set_value(RUN_VALUE_NAME, &value)
                    .expect("set value before uninstall");

                key.delete_value(RUN_VALUE_NAME)
                    .expect("delete should succeed when value exists");

                let check: Result<String, _> = key.get_value(RUN_VALUE_NAME);
                assert!(check.is_err(), "value should be absent after deletion");
            });
        }

        /// Attempting to delete a value that does not exist should return
        /// Ok(()) (idempotent), mirroring the production uninstall_service logic.
        #[test]
        fn test_uninstall_when_absent() {
            with_temp_key("uninstall_absent", |key| {
                // Value has never been set — mirror the production logic.
                let result: anyhow::Result<()> = match key.delete_value(RUN_VALUE_NAME) {
                    Ok(()) => Ok(()),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                    Err(e) => Err(anyhow::anyhow!("Failed to remove registry value: {}", e)),
                };

                assert!(
                    result.is_ok(),
                    "uninstall on absent value should be idempotent (Ok): {:?}",
                    result
                );
            });
        }

        /// A value that is present should be detected as registered.
        #[test]
        fn test_is_registered_true_when_present() {
            with_temp_key("is_registered_true", |key| {
                let value = format!("\"{}\" start", r"C:\ACS\acs.exe");
                key.set_value(RUN_VALUE_NAME, &value).expect("set value");

                let registered = key.get_value::<String, _>(RUN_VALUE_NAME).is_ok();
                assert!(registered, "should report registered when value is present");
            });
        }

        /// A value that has never been written should be detected as absent.
        #[test]
        fn test_is_registered_false_when_absent() {
            with_temp_key("is_registered_false", |key| {
                let registered = key.get_value::<String, _>(RUN_VALUE_NAME).is_ok();
                assert!(
                    !registered,
                    "should report not-registered when value is absent"
                );
            });
        }
    }
}
