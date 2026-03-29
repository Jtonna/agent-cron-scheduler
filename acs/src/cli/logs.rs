// CLI log commands: logs

use std::io::{self, Write};

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

/// acs logs
#[allow(clippy::too_many_arguments)]
pub async fn cmd_logs(
    host: &str,
    port: u16,
    job: &str,
    follow: bool,
    run: Option<&str>,
    last: Option<usize>,
    tail: Option<usize>,
    json: bool,
) -> anyhow::Result<()> {
    let client = Client::new();

    if follow {
        // Follow mode: first resolve job ID, then stream SSE
        let job_id = resolve_job_id(&client, host, port, job).await?;
        let sse_url = format!("{}/api/events?job_id={}", base_url(host, port), job_id);
        follow_sse_logs(&client, &sse_url).await?;
        return Ok(());
    }

    if let Some(run_id) = run {
        // Show a specific run's log
        show_run_log(&client, host, port, run_id, tail, json).await?;
    } else {
        // List runs (optionally limited by --last)
        let limit = last.unwrap_or(20);
        list_runs(&client, host, port, job, limit, json).await?;
    }

    Ok(())
}

/// Resolve a job name/UUID to a job ID string.
async fn resolve_job_id(
    client: &Client,
    host: &str,
    port: u16,
    job: &str,
) -> anyhow::Result<String> {
    let url = format!("{}/api/jobs/{}", base_url(host, port), job);

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
        let message = body["message"].as_str().unwrap_or("Job not found");
        anyhow::bail!("Error: {}", message);
    }

    let job_id = body["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing job ID in response"))?;

    Ok(job_id.to_string())
}

/// List runs for a job.
async fn list_runs(
    client: &Client,
    host: &str,
    port: u16,
    job: &str,
    limit: usize,
    json: bool,
) -> anyhow::Result<()> {
    let url = format!(
        "{}/api/jobs/{}/runs?limit={}&offset=0",
        base_url(host, port),
        job,
        limit
    );

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
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&body)?);
        return Ok(());
    }

    let runs = body["runs"].as_array();
    let total = body["total"].as_u64().unwrap_or(0);

    match runs {
        Some(runs) if !runs.is_empty() => {
            println!(
                "Showing {} of {} runs for job '{}':\n",
                runs.len(),
                total,
                job
            );

            // Print header
            println!(
                "{:<38}{:<22}{:<12}{:<10}{:<12}",
                "RUN ID", "STARTED", "STATUS", "EXIT", "SIZE"
            );

            for run in runs {
                let run_id = run["run_id"].as_str().unwrap_or("?");
                let started = run["started_at"].as_str().unwrap_or("?");
                let run_status = run["status"].as_str().unwrap_or("?");
                let exit_code = match run["exit_code"].as_i64() {
                    Some(c) => c.to_string(),
                    None => "-".to_string(),
                };
                let log_size = run["log_size_bytes"].as_u64().unwrap_or(0);

                // Format started time (show just the datetime part)
                let started_display = if started.len() > 19 {
                    &started[..19]
                } else {
                    started
                };

                println!(
                    "{:<38}{:<22}{:<12}{:<10}{:<12}",
                    run_id,
                    started_display,
                    run_status,
                    exit_code,
                    format_bytes(log_size)
                );
            }
        }
        _ => {
            println!("No runs found for job '{}'.", job);
        }
    }

    Ok(())
}

/// Show a specific run's log output.
async fn show_run_log(
    client: &Client,
    host: &str,
    port: u16,
    run_id: &str,
    tail: Option<usize>,
    json: bool,
) -> anyhow::Result<()> {
    let mut url = format!("{}/api/runs/{}/log", base_url(host, port), run_id);

    if let Some(n) = tail {
        url.push_str(&format!("?tail={}", n));
    }

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();

    if !status.is_success() {
        let body: Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({"message": "Unknown error"}));
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    let body = response
        .text()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to read response: {}", e))?;

    if json {
        // Wrap log content in JSON
        let output = serde_json::json!({
            "run_id": run_id,
            "log": body,
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        print!("{}", body);
    }

    Ok(())
}

/// Follow an SSE stream for log output.
///
/// The SSE `data:` field contains the full serde-tagged JSON:
///   `{"event":"Output","data":{"job_id":"...","run_id":"...","data":"the output","timestamp":"..."}}`
/// So after parsing the JSON, the output text is at `json["data"]["data"]`,
/// the job_name is at `json["data"]["job_name"]`, etc.
async fn follow_sse_logs(client: &Client, url: &str) -> anyhow::Result<()> {
    use futures_util::StreamExt;

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to connect to SSE stream: {}", e))?;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    println!("Following log output (Ctrl+C to stop)...\n");

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!("SSE stream error: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Parse SSE events from buffer
        while let Some(pos) = buffer.find("\n\n") {
            let event_block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            let mut event_type = String::new();
            let mut data = String::new();

            for line in event_block.lines() {
                if let Some(rest) = line.strip_prefix("event: ") {
                    event_type = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    data = rest.to_string();
                }
            }

            match event_type.as_str() {
                "started" => {
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        let job_name = json["data"]["job_name"].as_str().unwrap_or("unknown");
                        let run_id = json["data"]["run_id"].as_str().unwrap_or("unknown");
                        println!("--- Job '{}' started (run: {}) ---", job_name, run_id);
                    }
                }
                "output" => {
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        if let Some(output) = json["data"]["data"].as_str() {
                            print!("{}", output);
                            io::stdout().flush()?;
                        }
                    }
                }
                "completed" => {
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        let exit_code = json["data"]["exit_code"]
                            .as_i64()
                            .map(|c| c.to_string())
                            .unwrap_or_else(|| "?".to_string());
                        println!("\n--- Job completed (exit code: {}) ---", exit_code);
                    }
                }
                "failed" => {
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        let error = json["data"]["error"].as_str().unwrap_or("unknown error");
                        eprintln!("\n--- Job failed: {} ---", error);
                    }
                }
                "keepalive" | "" => {
                    // Ignore keepalive comments
                }
                _ => {}
            }
        }
    }

    Ok(())
}

/// Format byte size into a human-readable string.
fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "0 B".to_string()
    } else if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    }
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_zero() {
        assert_eq!(format_bytes(0), "0 B");
    }

    #[test]
    fn test_format_bytes_bytes() {
        assert_eq!(format_bytes(512), "512 B");
    }

    #[test]
    fn test_format_bytes_kilobytes() {
        let result = format_bytes(1536);
        assert!(result.contains("KB"), "Got: {}", result);
    }

    #[test]
    fn test_format_bytes_megabytes() {
        let result = format_bytes(2 * 1024 * 1024);
        assert!(result.contains("MB"), "Got: {}", result);
    }

    #[test]
    fn test_format_bytes_exact_kb() {
        assert_eq!(format_bytes(1024), "1.0 KB");
    }

    #[test]
    fn test_format_bytes_exact_mb() {
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    }

    #[tokio::test]
    async fn test_resolve_job_id_connection_error() {
        let client = Client::new();
        let result = resolve_job_id(&client, "127.0.0.1", 1, "test-job").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Could not connect") || err.contains("Request failed"),
            "Got: {}",
            err
        );
    }
}
