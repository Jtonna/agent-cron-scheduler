// CLI job commands: add, remove, list, enable, disable, trigger

use std::io::{self, BufRead, Write};

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde_json::Value;

use super::{base_url, connection_error_message, parse_env_vars};
use crate::models::job::ExecutionType;
use crate::models::NewJob;

/// Helper to handle reqwest errors and produce a user-friendly connection error.
fn handle_request_error(err: reqwest::Error, host: &str, port: u16) -> anyhow::Error {
    if err.is_connect() || err.is_timeout() {
        anyhow::anyhow!("{}", connection_error_message(host, port))
    } else {
        anyhow::anyhow!("Request failed: {}", err)
    }
}

/// Format a relative time string like "2 minutes ago" or "in 3 minutes".
fn format_relative_time(dt: &DateTime<Utc>) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(*dt);

    if diff.num_seconds() < 0 {
        // Future time
        let abs = -diff.num_seconds();
        if abs < 60 {
            format!("in {} seconds", abs)
        } else if abs < 3600 {
            format!("in {} minutes", abs / 60)
        } else if abs < 86400 {
            format!("in {} hours", abs / 3600)
        } else {
            format!("in {} days", abs / 86400)
        }
    } else {
        let secs = diff.num_seconds();
        if secs < 60 {
            format!("{} seconds ago", secs)
        } else if secs < 3600 {
            format!("{} minutes ago", secs / 60)
        } else if secs < 86400 {
            format!("{} hours ago", secs / 3600)
        } else {
            format!("{} days ago", secs / 86400)
        }
    }
}

/// acs add
#[allow(clippy::too_many_arguments)]
pub async fn cmd_add(
    host: &str,
    port: u16,
    name: &str,
    schedule: &str,
    cmd: Option<&str>,
    script: Option<&str>,
    timezone: Option<&str>,
    working_dir: Option<&str>,
    env: &[String],
    disabled: bool,
    log_env: bool,
) -> anyhow::Result<()> {
    let execution = match (cmd, script) {
        (Some(c), None) => ExecutionType::ShellCommand(c.to_string()),
        (None, Some(s)) => ExecutionType::ScriptFile(s.to_string()),
        _ => {
            anyhow::bail!("Either --cmd (-c) or --script must be specified");
        }
    };

    let env_vars = if env.is_empty() {
        None
    } else {
        let parsed = parse_env_vars(env).map_err(|e| anyhow::anyhow!(e))?;
        Some(parsed)
    };

    let new_job = NewJob {
        name: name.to_string(),
        schedule: schedule.to_string(),
        execution,
        enabled: !disabled,
        timezone: timezone.map(|s| s.to_string()),
        working_dir: working_dir.map(|s| s.to_string()),
        env_vars,
        timeout_secs: 0,
        log_environment: log_env,
    };

    let client = Client::new();
    let url = format!("{}/api/jobs", base_url(host, port));

    let response = client
        .post(&url)
        .json(&new_job)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    if status.is_success() {
        println!("Job '{}' created successfully.", name);
        println!("  ID:       {}", body["id"].as_str().unwrap_or("unknown"));
        println!("  Schedule: {}", schedule);
        println!("  Enabled:  {}", !disabled);
    } else {
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    Ok(())
}

/// acs remove
pub async fn cmd_remove(host: &str, port: u16, job: &str, yes: bool) -> anyhow::Result<()> {
    if !yes {
        print!("Are you sure you want to remove job '{}'? [y/N] ", job);
        io::stdout().flush()?;
        let stdin = io::stdin();
        let mut line = String::new();
        stdin.lock().read_line(&mut line)?;
        let answer = line.trim().to_lowercase();
        if answer != "y" && answer != "yes" {
            println!("Cancelled.");
            return Ok(());
        }
    }

    let client = Client::new();
    let url = format!("{}/api/jobs/{}", base_url(host, port), job);

    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();

    if status.as_u16() == 204 {
        println!("Job '{}' removed.", job);
    } else {
        let body: Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({"message": "Unknown error"}));
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    Ok(())
}

/// acs list
pub async fn cmd_list(
    host: &str,
    port: u16,
    enabled: bool,
    disabled: bool,
    json: bool,
) -> anyhow::Result<()> {
    let client = Client::new();
    let mut url = format!("{}/api/jobs", base_url(host, port));

    if enabled {
        url.push_str("?enabled=true");
    } else if disabled {
        url.push_str("?enabled=false");
    }

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

    // Pretty table output
    let empty_vec = vec![];
    let jobs = body.as_array().unwrap_or(&empty_vec);

    if jobs.is_empty() {
        println!("No jobs found.");
        return Ok(());
    }

    // Print header
    println!(
        "{:<14}{:<16}{:<10}{:<18}{:<18}{:<10}",
        "NAME", "SCHEDULE", "ENABLED", "LAST RUN", "NEXT RUN", "LAST EXIT"
    );

    for job in jobs {
        let name = job["name"].as_str().unwrap_or("?");
        let schedule = job["schedule"].as_str().unwrap_or("?");
        let job_enabled = job["enabled"].as_bool().unwrap_or(false);

        let last_run = match job["last_run_at"].as_str() {
            Some(ts) => {
                if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                    format_relative_time(&dt)
                } else {
                    "-".to_string()
                }
            }
            None => "-".to_string(),
        };

        let next_run = match job["next_run_at"].as_str() {
            Some(ts) => {
                if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                    format_relative_time(&dt)
                } else {
                    "-".to_string()
                }
            }
            None => "-".to_string(),
        };

        let last_exit = match job["last_exit_code"].as_i64() {
            Some(code) => code.to_string(),
            None => "-".to_string(),
        };

        // Truncate name if too long
        let display_name = if name.len() > 13 {
            format!("{}...", &name[..10])
        } else {
            name.to_string()
        };

        println!(
            "{:<14}{:<16}{:<10}{:<18}{:<18}{:<10}",
            display_name,
            if schedule.len() > 15 {
                format!("{}...", &schedule[..12])
            } else {
                schedule.to_string()
            },
            job_enabled,
            last_run,
            next_run,
            last_exit
        );
    }

    Ok(())
}

/// acs enable
pub async fn cmd_enable(host: &str, port: u16, job: &str) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/api/jobs/{}/enable", base_url(host, port), job);

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
        let name = body["name"].as_str().unwrap_or(job);
        println!("Job '{}' enabled.", name);
    } else {
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    Ok(())
}

/// acs disable
pub async fn cmd_disable(host: &str, port: u16, job: &str) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/api/jobs/{}/disable", base_url(host, port), job);

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
        let name = body["name"].as_str().unwrap_or(job);
        println!("Job '{}' disabled.", name);
    } else {
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    Ok(())
}

/// acs trigger
pub async fn cmd_trigger(host: &str, port: u16, job: &str, follow: bool) -> anyhow::Result<()> {
    let client = Client::new();

    if follow {
        // To avoid a race condition where fast jobs complete before the SSE
        // connection is ready, we must establish the SSE stream FIRST, then
        // trigger the job.

        // 1. Resolve job to get the job_id
        let job_id = resolve_job_id(&client, host, port, job).await?;

        // 2. Open SSE connection BEFORE triggering
        let sse_url = format!("{}/api/events?job_id={}", base_url(host, port), job_id);
        let sse_response = client
            .get(&sse_url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to SSE stream: {}", e))?;

        // 3. Now trigger the job
        let trigger_url = format!("{}/api/jobs/{}/trigger", base_url(host, port), job);
        let response = client
            .post(&trigger_url)
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

        let job_name = body["job_name"].as_str().unwrap_or(job);
        println!("Job '{}' triggered.", job_name);

        // 4. Read SSE events from the already-connected stream
        follow_sse_stream(sse_response).await?;
    } else {
        let url = format!("{}/api/jobs/{}/trigger", base_url(host, port), job);

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

        if !status.is_success() {
            let message = body["message"].as_str().unwrap_or("Unknown error");
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }

        let job_name = body["job_name"].as_str().unwrap_or(job);
        println!("Job '{}' triggered.", job_name);
    }

    Ok(())
}

/// Resolve a job name or UUID to a job ID string.
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

/// Follow an SSE stream from an already-connected response and print events to stdout.
///
/// The SSE `data:` field contains the full serde-tagged JSON:
///   `{"event":"Output","data":{"job_id":"...","run_id":"...","data":"the output","timestamp":"..."}}`
/// So after parsing the JSON, the output text is at `json["data"]["data"]`,
/// exit code at `json["data"]["exit_code"]`, etc.
async fn follow_sse_stream(response: reqwest::Response) -> anyhow::Result<()> {
    use futures_util::StreamExt;

    let mut stream = response.bytes_stream();
    let mut buffer = String::new();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| anyhow::anyhow!("SSE stream error: {}", e))?;
        let text = String::from_utf8_lossy(&chunk);
        buffer.push_str(&text);

        // Parse SSE events from buffer
        while let Some(pos) = buffer.find("\n\n") {
            let event_block = buffer[..pos].to_string();
            buffer = buffer[pos + 2..].to_string();

            // Parse the event
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
                "output" => {
                    // data field is: {"event":"Output","data":{"job_id":"...","data":"the text",...}}
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        if let Some(output) = json["data"]["data"].as_str() {
                            print!("{}", output);
                            io::stdout().flush()?;
                        }
                    }
                }
                "completed" => {
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        if let Some(exit_code) = json["data"]["exit_code"].as_i64() {
                            println!("\n--- Job finished (exit code: {}) ---", exit_code);
                        } else {
                            println!("\n--- Job finished ---");
                        }
                    }
                    return Ok(());
                }
                "failed" => {
                    if let Ok(json) = serde_json::from_str::<Value>(&data) {
                        if let Some(error) = json["data"]["error"].as_str() {
                            eprintln!("\n--- Job failed: {} ---", error);
                        } else {
                            eprintln!("\n--- Job failed ---");
                        }
                    }
                    return Ok(());
                }
                "keepalive" | "" => {
                    // Ignore keepalive comments
                }
                _ => {
                    // Other events (started, job_changed), ignore silently
                }
            }
        }
    }

    Ok(())
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_relative_time_past() {
        let past = Utc::now() - chrono::Duration::minutes(5);
        let result = format_relative_time(&past);
        assert!(result.contains("minutes ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_future() {
        let future = Utc::now() + chrono::Duration::hours(2);
        let result = format_relative_time(&future);
        assert!(result.contains("in"), "Got: {}", result);
        assert!(result.contains("hours"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_seconds_ago() {
        let past = Utc::now() - chrono::Duration::seconds(30);
        let result = format_relative_time(&past);
        assert!(result.contains("seconds ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_hours_ago() {
        let past = Utc::now() - chrono::Duration::hours(10);
        let result = format_relative_time(&past);
        assert!(result.contains("hours ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_days_ago() {
        let past = Utc::now() - chrono::Duration::days(3);
        let result = format_relative_time(&past);
        assert!(result.contains("days ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_in_seconds() {
        let future = Utc::now() + chrono::Duration::seconds(45);
        let result = format_relative_time(&future);
        assert!(
            result.contains("in") && result.contains("seconds"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_format_relative_time_in_minutes() {
        let future = Utc::now() + chrono::Duration::minutes(15);
        let result = format_relative_time(&future);
        assert!(
            result.contains("in") && result.contains("minutes"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_format_relative_time_in_days() {
        let future = Utc::now() + chrono::Duration::days(2);
        let result = format_relative_time(&future);
        assert!(
            result.contains("in") && result.contains("days"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_handle_request_error_connection() {
        // Construct a connect error by trying to connect to an invalid address
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let client = Client::builder()
                .timeout(std::time::Duration::from_millis(1))
                .build()
                .unwrap();
            let result = client.get("http://192.0.2.1:1").send().await;
            if let Err(e) = result {
                let err = handle_request_error(e, "192.0.2.1", 1);
                let msg = err.to_string();
                // On timeout, it should show the connection error message
                assert!(
                    msg.contains("Could not connect") || msg.contains("Request failed"),
                    "Got: {}",
                    msg
                );
            }
        });
    }
}
