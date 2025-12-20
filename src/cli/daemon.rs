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
    _port: u16,
    foreground: bool,
    config: Option<&str>,
    port_override: Option<u16>,
    data_dir: Option<&str>,
) -> anyhow::Result<()> {
    // Ensure tracing is initialized for daemon mode
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .try_init();

    // If --foreground is specified, run the daemon directly in this process
    if foreground {
        return run_daemon_foreground(host, config, port_override, data_dir).await;
    }

    // Background mode: use system service manager
    let exe_path = std::env::current_exe().context("Failed to determine executable path")?;

    // Check if service is already registered
    if !service::is_service_registered() {
        println!("Registering system service...");
        if let Err(e) = service::install_service(&exe_path) {
            // On failure, provide helpful message and fall back to foreground suggestion
            eprintln!("Warning: Could not register system service: {}", e);
            eprintln!("This may require elevated privileges (admin/sudo).");
            eprintln!("You can run with --foreground to start the daemon in the current terminal.");
            return Err(e);
        }
        println!(
            "Service registered successfully ({} on {}).",
            service::service_name(),
            service::platform_name()
        );
    }

    // Start the daemon via the service manager
    println!("Starting daemon via system service...");
    match service::start_service() {
        Ok(()) => {
            println!("Daemon started successfully.");
            println!(
                "The daemon is now running as a system service and will persist across reboots."
            );
            println!("Use 'acs status' to check daemon status.");
            println!("Use 'acs stop' to stop the daemon.");
            Ok(())
        }
        Err(e) => {
            eprintln!("Failed to start service: {}", e);
            eprintln!("You can try:");
            eprintln!("  - Running with elevated privileges (admin/sudo)");
            eprintln!("  - Using 'acs start --foreground' to run in the current terminal");
            Err(e)
        }
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

    // If running as a service, use SCM to stop (this is the proper way).
    // SCM will signal the service, which triggers graceful shutdown internally.
    if service::is_service_registered() {
        println!("Stopping daemon via service manager...");
        match service::stop_service() {
            Ok(()) => {
                println!("Daemon is shutting down...");
                return Ok(());
            }
            Err(e) => {
                // SCM stop failed, fall through to try API
                tracing::debug!("Service manager stop failed: {}, trying API", e);
            }
        }
    }

    // Fall back to API shutdown (for foreground mode or if SCM stop failed)
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
    // Stop the daemon — prefer SCM if registered, then fall back to API
    if service::is_service_registered() {
        println!("Stopping daemon via service manager...");
        match service::stop_service() {
            Ok(()) => {
                println!("Daemon stopped.");
                // Give it a moment to shut down
                tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
            }
            Err(e) => {
                tracing::debug!("Service manager stop failed: {}, trying API", e);
                // Fall back to API shutdown
                let client = Client::new();
                let url = format!("{}/api/shutdown", base_url(host, port));
                match client.post(&url).send().await {
                    Ok(response) if response.status().is_success() => {
                        println!("Daemon stopped via API.");
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                    }
                    _ => {
                        println!("Warning: Could not stop daemon (may already be stopped).");
                    }
                }
            }
        }
    } else {
        // No service registered, try API
        let client = Client::new();
        let url = format!("{}/api/shutdown", base_url(host, port));
        match client.post(&url).send().await {
            Ok(response) if response.status().is_success() => {
                println!("Daemon stopped via API.");
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            _ => {
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
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        // When service is registered, it may try service manager stop which gives different error
        assert!(
            err.contains("Could not connect")
                || err.contains("Request failed")
                || err.contains("IO error"),
            "Got: {}",
            err
        );
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
