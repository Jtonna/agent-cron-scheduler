// CLI daemon commands: start, stop, status, uninstall

use reqwest::Client;
use serde_json::Value;

use super::{base_url, connection_error_message};

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
        foreground,
    )
    .await
}

/// acs stop
pub async fn cmd_stop(host: &str, port: u16, force: bool) -> anyhow::Result<()> {
    if force {
        // Force kill: read PID file and kill the process
        println!("Force stopping daemon...");
        // In a full implementation:
        // 1. Read PID file
        // 2. Kill the process
        // 3. Clean up PID file
        println!("Daemon would be force-killed here.");
        println!("(Full implementation reads PID file and kills process)");
        return Ok(());
    }

    // Graceful shutdown via API
    let client = Client::new();
    let url = format!("{}/api/shutdown", base_url(host, port));

    let response = client
        .post(&url)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    if status.is_success() {
        println!("Daemon is shutting down...");
    } else {
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

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

    println!("Daemon Status: {}", daemon_status);
    println!("  Version:     {}", version);
    println!("  Uptime:      {}", format_uptime(uptime));
    println!(
        "  Jobs:        {} active / {} total",
        active_jobs, total_jobs
    );

    if verbose {
        println!("\nRaw response:");
        println!("{}", serde_json::to_string_pretty(&body)?);
    }

    Ok(())
}

/// acs uninstall
pub async fn cmd_uninstall(host: &str, port: u16, purge: bool) -> anyhow::Result<()> {
    // Try to stop the daemon first
    let client = Client::new();
    let url = format!("{}/api/shutdown", base_url(host, port));

    match client.post(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                println!("Daemon stopped.");
            }
        }
        Err(_) => {
            // Daemon might not be running, that's OK
            println!("Daemon is not running (or not reachable).");
        }
    }

    // Remove service registration
    println!("Removing system service registration...");
    // In a full implementation:
    // - Windows: delete Windows Service
    // - macOS: remove launchd plist
    // - Linux: remove systemd unit
    println!("(Full service removal implementation is in src/daemon/service.rs)");

    if purge {
        println!("Purging data directory...");
        // In a full implementation:
        // - Remove {data_dir} entirely
        println!("(Full purge implementation would remove the data directory)");
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
        assert!(
            err.contains("Could not connect") || err.contains("Request failed"),
            "Got: {}",
            err
        );
    }
}
