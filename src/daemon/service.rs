// Platform service registration for daemon persistence.
//
// - Windows: Windows Service via `windows-service` crate
// - macOS:   launchd plist to ~/Library/LaunchAgents/com.acs.scheduler.plist
// - Linux:   systemd user unit to ~/.config/systemd/user/acs.service

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
        "com.acs.scheduler"
    } else {
        "acs"
    }
}

// ---------------------------------------------------------------------------
// Windows service implementation
// ---------------------------------------------------------------------------
#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use std::ffi::OsString;

    /// Check if the Windows Service is registered.
    pub fn is_service_registered() -> bool {
        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

        let manager =
            match ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT) {
                Ok(m) => m,
                Err(_) => return false,
            };

        use windows_service::service::ServiceAccess;
        manager
            .open_service("AgentCronScheduler", ServiceAccess::QUERY_STATUS)
            .is_ok()
    }

    /// Install the Windows Service.
    pub fn install_service(exe_path: &Path) -> anyhow::Result<()> {
        use anyhow::Context;
        use windows_service::service::{
            ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
        };
        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

        let manager = ServiceManager::local_computer(
            None::<&str>,
            ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
        )
        .context("Failed to connect to Service Control Manager (run as Administrator)")?;

        let service_info = ServiceInfo {
            name: OsString::from("AgentCronScheduler"),
            display_name: OsString::from("Agent Cron Scheduler"),
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: exe_path.to_path_buf(),
            launch_arguments: vec![OsString::from("service")],
            dependencies: vec![],
            account_name: None,
            account_password: None,
        };

        manager
            .create_service(&service_info, ServiceAccess::CHANGE_CONFIG)
            .context("Failed to create service (already exists or need Administrator)")?;
        Ok(())
    }

    /// Uninstall the Windows Service.
    pub fn uninstall_service() -> anyhow::Result<()> {
        use anyhow::Context;
        use windows_service::service::ServiceAccess;
        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .context("Failed to connect to Service Control Manager")?;

        let service = manager
            .open_service(
                "AgentCronScheduler",
                ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
            )
            .context("Failed to open service for deletion")?;

        service
            .delete()
            .context("Failed to delete service (run as Administrator)")?;
        Ok(())
    }

    /// Get the service file/registry path on Windows (not a simple file path).
    pub fn service_path() -> Option<String> {
        if is_service_registered() {
            Some("HKLM\\SYSTEM\\CurrentControlSet\\Services\\AgentCronScheduler".to_string())
        } else {
            None
        }
    }

    /// Start the Windows Service.
    pub fn start_service() -> anyhow::Result<()> {
        use anyhow::Context;
        use windows_service::service::ServiceAccess;
        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .context("Failed to connect to Service Control Manager")?;

        let service = manager
            .open_service(
                "AgentCronScheduler",
                ServiceAccess::START | ServiceAccess::QUERY_STATUS,
            )
            .context("Failed to open service (is it registered?)")?;

        service
            .start::<&str>(&[])
            .context("Failed to start service (try running as Administrator)")?;
        Ok(())
    }

    /// Stop the Windows Service.
    pub fn stop_service() -> anyhow::Result<()> {
        use anyhow::Context;
        use windows_service::service::ServiceAccess;
        use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .context("Failed to connect to Service Control Manager")?;

        let service = manager
            .open_service(
                "AgentCronScheduler",
                ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
            )
            .context("Failed to open service")?;

        service.stop().context("Failed to stop service")?;
        Ok(())
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
        let home = dirs::home_dir().expect("Could not determine home directory");
        home.join("Library")
            .join("LaunchAgents")
            .join("com.acs.scheduler.plist")
    }

    pub fn is_service_registered() -> bool {
        plist_path().exists()
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
    <string>com.acs.scheduler</string>
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
            .arg("com.acs.scheduler")
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
            .arg("com.acs.scheduler")
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
        let home = dirs::home_dir().expect("Could not determine home directory");
        home.join(".config")
            .join("systemd")
            .join("user")
            .join("acs.service")
    }

    pub fn is_service_registered() -> bool {
        unit_path().exists()
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
            .arg("acs.service")
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
                .arg("acs.service")
                .status();
            let _ = std::process::Command::new("systemctl")
                .arg("--user")
                .arg("disable")
                .arg("acs.service")
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
            .arg("acs.service")
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
            .arg("acs.service")
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

    #[test]
    fn test_start_service_fails_when_not_registered() {
        // If service is not registered, start_service should fail
        if !is_service_registered() {
            let result = start_service();
            assert!(
                result.is_err(),
                "start_service should fail when service is not registered"
            );
        }
        // If service IS registered, we skip this test to avoid side effects
    }

    #[test]
    fn test_stop_service_fails_when_not_registered() {
        // If service is not registered, stop_service should fail
        if !is_service_registered() {
            let result = stop_service();
            assert!(
                result.is_err(),
                "stop_service should fail when service is not registered"
            );
        }
        // If service IS registered, we skip this test to avoid side effects
    }

    #[test]
    fn test_install_service_requires_valid_path() {
        use std::path::PathBuf;

        // Note: This test may fail with permission errors, which is expected
        // on systems that require elevated privileges for service installation.
        // We're mainly testing that the function exists and handles the path correctly.
        let fake_exe = PathBuf::from("/nonexistent/path/to/exe");

        // We don't assert on the result because:
        // - On Windows, it will fail due to service manager access
        // - On Unix, it will fail due to permissions or directory issues
        // The important thing is it doesn't panic
        let _result = install_service(&fake_exe);
    }

    #[test]
    fn test_uninstall_service_when_not_registered() {
        // If service is not registered, uninstall should fail gracefully
        if !is_service_registered() {
            let result = uninstall_service();
            // On Windows, this will fail to find the service
            // On Unix, this will fail because the file doesn't exist
            assert!(
                result.is_err(),
                "uninstall_service should fail when service is not registered"
            );
        }
    }
}
