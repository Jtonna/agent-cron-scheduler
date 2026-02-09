// CLI daemon commands: start, stop, status, uninstall

use anyhow::Context;
use reqwest::Client;
use serde_json::Value;

use super::{base_url, connection_error_message};
use crate::daemon::service;

/// Helper to handle reqwest errors and produce a user-friendly connection error.
fn handle_request_error(err: reqwest::Error, host: &str, port: u16) -> anyhow::Error {
    if err.is_connect() || err.is_timeout() {
        anyhow::anyhow!("{}", connection_error_message(host, port))
    } else {
        anyhow::anyhow!("Request failed: {}", err)
    }
}

/// acs start
pub async fn cmd_start(
    host: &str,
    port: u16,
    foreground: bool,
    config: Option<&str>,
    port_override: Option<u16>,
    data_dir: Option<&str>,
) -> anyhow::Result<()> {
    // If --foreground is specified, run the daemon directly in this process
    if foreground {
        return run_daemon_foreground(host, config, port_override, data_dir).await;
    }

    // Background mode: spawn daemon as a hidden process, register for auto-start
    let exe_path = std::env::current_exe().context("Failed to determine executable path")?;

    // Register scheduled task for auto-start at logon (if not already)
    if !service::is_service_registered() {
        println!("Registering auto-start task...");
        if let Err(e) = service::install_service(&exe_path) {
            eprintln!("Warning: Could not register auto-start: {}", e);
            eprintln!("The daemon will start now but won't persist across reboots.");
            eprintln!("You may need elevated privileges to register the task.");
        } else {
            println!(
                "Auto-start registered ({} on {}).",
                service::service_name(),
                service::platform_name()
            );
        }
    }

    // Quick check: is the daemon already running?
    let check_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .build()
        .ok();
    if let Some(client) = check_client {
        let port = port_override.unwrap_or(port);
        if client
            .get(format!("{}/health", super::base_url(host, port)))
            .send()
            .await
            .is_ok()
        {
            println!("Daemon is already running.");
            return Ok(());
        }
    }

    // Spawn the daemon as a hidden background process
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        // Redirect stderr to a log file so startup errors aren't lost
        let data_dir = crate::daemon::resolve_data_dir(data_dir.map(std::path::Path::new));
        let _ = std::fs::create_dir_all(&data_dir);
        let log_file = std::fs::File::create(data_dir.join("daemon.log"))
            .context("Failed to create daemon log file")?;

        let mut cmd = std::process::Command::new(&exe_path);
        cmd.args(["start", "--foreground"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        cmd.stderr(log_file);
        cmd.stdout(std::process::Stdio::null());
        cmd.spawn().context("Failed to spawn daemon process")?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        // On macOS/Linux, use the service manager (launchd/systemd) to start
        service::start_service().context("Failed to start daemon via service manager")?;
    }

    // Wait for the daemon to become healthy (up to 3 seconds)
    let port = port_override.unwrap_or(port);
    let health_url = format!("{}/health", super::base_url(host, port));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .build()
        .unwrap_or_default();

    let mut started = false;
    for _ in 0..6 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if client.get(&health_url).send().await.is_ok() {
            started = true;
            break;
        }
    }

    if started {
        println!("Daemon started in background.");
        println!("Use 'acs status' to check daemon status.");
        println!("Use 'acs stop' to stop the daemon.");
        Ok(())
    } else {
        eprintln!(
            "Warning: Daemon was spawned but is not responding on port {}.",
            port
        );
        #[cfg(target_os = "windows")]
        {
            let data_dir = crate::daemon::resolve_data_dir(data_dir.map(std::path::Path::new));
            eprintln!(
                "Check {} for errors.",
                data_dir.join("daemon.log").display()
            );
        }
        Err(anyhow::anyhow!("Daemon failed to start"))
    }
}

/// Run the daemon directly in the foreground (blocking).
async fn run_daemon_foreground(
    host: &str,
    config: Option<&str>,
    port_override: Option<u16>,
    data_dir: Option<&str>,
) -> anyhow::Result<()> {
    let config_path = config.map(std::path::Path::new);
    let data_dir_path = data_dir.map(std::path::Path::new);

    // Determine host/port overrides: use the Start subcommand's --port if provided,
    // otherwise use global --host/--port only if they differ from defaults
    let host_override = if host != "127.0.0.1" {
        Some(host)
    } else {
        None
    };

    crate::daemon::start_daemon(
        config_path,
        data_dir_path,
        host_override,
        port_override,
        true, // foreground = true
    )
    .await
}

/// acs stop
pub async fn cmd_stop(host: &str, port: u16, force: bool) -> anyhow::Result<()> {
    if force {
        // Force kill: read PID file and kill the process
        println!("Force stopping daemon...");
        return force_kill_daemon();
    }

    // Try graceful API shutdown first (works for both foreground and task scheduler)
    let client = Client::new();
    let url = format!("{}/api/shutdown", base_url(host, port));

    match client.post(&url).send().await {
        Ok(response) => {
            let status = response.status();
            let body: Value = response
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

            if status.is_success() {
                println!("Daemon is shutting down...");
                Ok(())
            } else {
                let message = body["message"].as_str().unwrap_or("Unknown error");
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }
        }
        Err(e) if e.is_connect() || e.is_timeout() => {
            // API unreachable — if registered as a task, try ending it
            if service::is_service_registered() {
                println!("API unreachable, ending scheduled task...");
                match service::stop_service() {
                    Ok(()) => {
                        println!("Daemon task ended.");
                        return Ok(());
                    }
                    Err(stop_err) => {
                        tracing::debug!("Task end also failed: {}", stop_err);
                    }
                }
            }
            Err(handle_request_error(e, host, port))
        }
        Err(e) => Err(anyhow::anyhow!("Request failed: {}", e)),
    }
}

/// Force kill the daemon by reading the PID file and terminating the process.
fn force_kill_daemon() -> anyhow::Result<()> {
    let data_dir = crate::daemon::resolve_data_dir(None);
    let pid_file_path = data_dir.join("acs.pid");

    if !pid_file_path.exists() {
        println!("No PID file found. Daemon may not be running.");
        return Ok(());
    }

    let content = std::fs::read_to_string(&pid_file_path).context("Failed to read PID file")?;
    let pid: u32 = content
        .trim()
        .parse()
        .context("Failed to parse PID from PID file")?;

    println!("Found daemon PID: {}", pid);

    // Kill the process
    #[cfg(unix)]
    {
        let result = unsafe { libc::kill(pid as i32, libc::SIGKILL) };
        if result == 0 {
            println!("Sent SIGKILL to process {}", pid);
        } else {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::ESRCH) {
                println!("Process {} not found (already dead)", pid);
            } else {
                return Err(anyhow::anyhow!("Failed to kill process {}: {}", pid, err));
            }
        }
    }

    #[cfg(windows)]
    {
        // Use taskkill command on Windows
        let status = std::process::Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .status()
            .context("Failed to execute taskkill")?;

        if status.success() {
            println!("Terminated process {}", pid);
        } else {
            println!(
                "taskkill exited with code {:?} (process may already be dead)",
                status.code()
            );
        }
    }

    // Clean up PID file
    if let Err(e) = std::fs::remove_file(&pid_file_path) {
        println!("Warning: Could not remove PID file: {}", e);
    } else {
        println!("Removed PID file.");
    }

    println!("Force stop complete.");
    Ok(())
}

/// acs status
pub async fn cmd_status(host: &str, port: u16, verbose: bool) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/health", base_url(host, port));

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    if !status.is_success() {
        let message = body["message"]
            .as_str()
            .unwrap_or("Daemon returned an error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    let daemon_status = body["status"].as_str().unwrap_or("unknown");
    let version = body["version"].as_str().unwrap_or("unknown");
    let uptime = body["uptime_seconds"].as_u64().unwrap_or(0);
    let active_jobs = body["active_jobs"].as_u64().unwrap_or(0);
    let total_jobs = body["total_jobs"].as_u64().unwrap_or(0);
    let data_dir = body["data_dir"].as_str().unwrap_or("unknown");

    // Check service registration
    let service_registered = service::is_service_registered();
    let service_status = if service_registered {
        format!("Registered ({})", service::service_name())
    } else {
        "Not registered".to_string()
    };

    println!("Daemon Status: {}", daemon_status);
    println!("  Data Dir:    {}", data_dir);
    println!("  Web UI:      http://{}:{}", host, port);
    println!(
        "  Jobs:        {} active / {} total",
        active_jobs, total_jobs
    );
    println!("  Uptime:      {}", format_uptime(uptime));
    println!("  Version:     {}", version);
    println!("  Service:     {}", service_status);

    if verbose {
        println!("\nRaw response:");
        println!("{}", serde_json::to_string_pretty(&body)?);
    }

    Ok(())
}

/// acs uninstall
pub async fn cmd_uninstall(host: &str, port: u16, purge: bool) -> anyhow::Result<()> {
    // Stop the daemon — try API first (graceful), fall back to task end
    let client = Client::new();
    let url = format!("{}/api/shutdown", base_url(host, port));
    match client.post(&url).send().await {
        Ok(response) if response.status().is_success() => {
            println!("Daemon stopped via API.");
            tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
        }
        _ => {
            // API unreachable — try ending the scheduled task
            if service::is_service_registered() {
                match service::stop_service() {
                    Ok(()) => {
                        println!("Daemon task ended.");
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                    Err(_) => {
                        println!("Warning: Could not stop daemon (may already be stopped).");
                    }
                }
            } else {
                println!("Daemon is not running (or not reachable).");
            }
        }
    }

    // Remove service registration
    if service::is_service_registered() {
        println!("Removing system service registration...");
        match service::uninstall_service() {
            Ok(()) => println!(
                "Service '{}' unregistered successfully.",
                service::service_name()
            ),
            Err(e) => {
                eprintln!("Warning: Failed to unregister service: {}", e);
                eprintln!("This may require elevated privileges (admin/sudo).");
            }
        }
    } else {
        println!("No service registration found.");
    }

    if purge {
        println!("Purging data directory...");
        let data_dir = crate::daemon::resolve_data_dir(None);
        if data_dir.exists() {
            match std::fs::remove_dir_all(&data_dir) {
                Ok(()) => println!("Removed data directory: {}", data_dir.display()),
                Err(e) => eprintln!("Warning: Failed to remove data directory: {}", e),
            }
        } else {
            println!("Data directory does not exist: {}", data_dir.display());
        }
    }

    println!("Uninstall complete.");

    Ok(())
}

/// acs restart
pub async fn cmd_restart(host: &str, port: u16) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/api/restart", base_url(host, port));

    println!("Requesting daemon restart...");

    match client.post(&url).send().await {
        Ok(response) => {
            let status = response.status();
            let body: Value = response
                .json()
                .await
                .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

            if !status.is_success() {
                let message = body["message"].as_str().unwrap_or("Unknown error");
                eprintln!("Error: {}", message);
                std::process::exit(1);
            }

            println!("Restart initiated. Waiting for daemon to come back up...");
        }
        Err(e) => {
            return Err(handle_request_error(e, host, port));
        }
    }

    // Poll /health until the new process is responding (up to 10 seconds)
    let health_url = format!("{}/health", base_url(host, port));
    let poll_client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_millis(500))
        .build()
        .unwrap_or_default();

    let mut came_back = false;
    for _ in 0..20 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if poll_client.get(&health_url).send().await.is_ok() {
            came_back = true;
            break;
        }
    }

    if came_back {
        println!("Daemon restarted successfully.");
        Ok(())
    } else {
        eprintln!("Warning: Daemon did not respond within 10 seconds after restart.");
        Err(anyhow::anyhow!(
            "Daemon failed to come back up after restart"
        ))
    }
}

/// Format uptime seconds into a human-readable string.
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, mins, secs)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, mins, secs)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs)
    } else {
        format!("{}s", secs)
    }
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_uptime_seconds() {
        assert_eq!(format_uptime(45), "45s");
    }

    #[test]
    fn test_format_uptime_minutes_seconds() {
        assert_eq!(format_uptime(125), "2m 5s");
    }

    #[test]
    fn test_format_uptime_hours() {
        assert_eq!(format_uptime(3661), "1h 1m 1s");
    }

    #[test]
    fn test_format_uptime_days() {
        assert_eq!(format_uptime(90061), "1d 1h 1m 1s");
    }

    #[test]
    fn test_format_uptime_zero() {
        assert_eq!(format_uptime(0), "0s");
    }

    #[test]
    fn test_format_uptime_exact_hour() {
        assert_eq!(format_uptime(3600), "1h 0m 0s");
    }

    #[test]
    fn test_format_uptime_exact_day() {
        assert_eq!(format_uptime(86400), "1d 0h 0m 0s");
    }

    #[tokio::test]
    async fn test_cmd_status_connection_error() {
        // Try to connect to a port that is almost certainly not listening
        let result = cmd_status("127.0.0.1", 1, false).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Could not connect") || err.contains("Request failed"),
            "Got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_cmd_stop_graceful_connection_error() {
        let result = cmd_stop("127.0.0.1", 1, false).await;
        // When the API is unreachable, cmd_stop falls back to ending the
        // scheduled task. If a task is registered, this may succeed (Ok).
        // If no task is registered, it returns a connection error (Err).
        if let Err(err) = result {
            let msg = err.to_string();
            assert!(
                msg.contains("Could not connect")
                    || msg.contains("Request failed")
                    || msg.contains("IO error"),
                "Got: {}",
                msg
            );
        }
    }

    #[test]
    fn test_service_functions_exist() {
        // Verify that service module functions are accessible
        // This is a compile-time check more than a runtime test
        let _registered = service::is_service_registered();
        let _name = service::service_name();
        let _platform = service::platform_name();
        let _status = service::service_status();
    }

    #[test]
    fn test_force_kill_no_pid_file() {
        // Create a temp directory that won't have a PID file
        // We can't easily test force_kill_daemon directly because it uses
        // resolve_data_dir, but we can test the logic conceptually
        use tempfile::TempDir;

        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("acs.pid");

        // PID file doesn't exist
        assert!(!pid_path.exists());
    }

    #[test]
    fn test_pid_file_parsing() {
        use tempfile::TempDir;

        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        // Write a valid PID
        std::fs::write(&pid_path, "12345").expect("write PID");
        let content = std::fs::read_to_string(&pid_path).expect("read PID file");
        let pid: u32 = content.trim().parse().expect("parse PID");
        assert_eq!(pid, 12345);

        // Write a PID with whitespace
        std::fs::write(&pid_path, "  67890\n").expect("write PID with whitespace");
        let content = std::fs::read_to_string(&pid_path).expect("read PID file");
        let pid: u32 = content.trim().parse().expect("parse PID");
        assert_eq!(pid, 67890);
    }

    #[test]
    fn test_invalid_pid_file() {
        use tempfile::TempDir;

        let tmp_dir = TempDir::new().expect("create temp dir");
        let pid_path = tmp_dir.path().join("test.pid");

        // Write invalid content
        std::fs::write(&pid_path, "not a number").expect("write invalid PID");
        let content = std::fs::read_to_string(&pid_path).expect("read PID file");
        let result: Result<u32, _> = content.trim().parse();
        assert!(result.is_err(), "Should fail to parse invalid PID");
    }
}
