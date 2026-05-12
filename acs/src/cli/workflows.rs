// CLI workflows subcommand: list, get, create, update, delete, trigger

use std::io::{self, BufRead, Write};

use chrono::{DateTime, Utc};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde_json::Value;

use super::{base_url, connection_error_message, parse_env_vars};

// ===========================================================================
// Clap subcommand definition
// ===========================================================================

/// Manage workflows
#[derive(Parser, Debug)]
pub struct WorkflowsCmd {
    #[command(subcommand)]
    pub subcommand: WorkflowsSubcommand,
}

#[derive(Subcommand, Debug)]
pub enum WorkflowsSubcommand {
    /// List all workflows
    List {
        /// Show only enabled workflows
        #[arg(long, conflicts_with = "disabled")]
        enabled: bool,

        /// Show only disabled workflows
        #[arg(long, conflicts_with = "enabled")]
        disabled: bool,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Show one workflow by id or name
    Get {
        /// Workflow id (UUID) or name
        id_or_name: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Create a workflow from a JSON file or inline JSON
    Create {
        /// Path to a JSON file containing the NewWorkflow definition
        #[arg(long, conflicts_with = "inline_json")]
        file: Option<String>,

        /// Inline JSON string containing the NewWorkflow definition
        #[arg(long = "json", conflicts_with = "file")]
        inline_json: Option<String>,
    },

    /// Update a workflow from a JSON file, inline JSON, or convenience flags
    Update {
        /// Workflow id (UUID) or name
        id_or_name: String,

        /// Path to a JSON file containing the WorkflowUpdate definition
        #[arg(long, conflicts_with = "inline_json")]
        file: Option<String>,

        /// Inline JSON string containing the WorkflowUpdate definition
        #[arg(long = "json", conflicts_with = "file")]
        inline_json: Option<String>,

        /// Convenience: set enabled=true
        #[arg(long, conflicts_with = "disable")]
        enable: bool,

        /// Convenience: set enabled=false
        #[arg(long, conflicts_with = "enable")]
        disable: bool,
    },

    /// Delete a workflow
    Delete {
        /// Workflow id (UUID) or name
        id_or_name: String,

        /// Skip confirmation prompt
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// Trigger a workflow run
    Trigger {
        /// Workflow id (UUID) or name
        id_or_name: String,

        /// Input JSON (any valid JSON value)
        #[arg(long, conflicts_with = "input_file")]
        input: Option<String>,

        /// Path to a file containing input JSON
        #[arg(long, conflicts_with = "input")]
        input_file: Option<String>,

        /// Environment variable overrides (KEY=VALUE), can be repeated
        #[arg(long = "env", short = 'e', value_name = "KEY=VALUE")]
        env: Vec<String>,

        /// Target step to start from
        #[arg(long)]
        target_step: Option<String>,

        /// After triggering, stream SSE events until RunCompleted or RunFailed
        #[arg(long)]
        follow: bool,
    },

    /// List runs for a workflow
    Runs {
        /// Workflow id (UUID) or name
        name_or_id: String,

        /// Max number of runs to return (default 20, max 100)
        #[arg(long, default_value_t = 20)]
        limit: usize,

        /// Skip the first N runs
        #[arg(long, default_value_t = 0)]
        offset: usize,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
}

// ===========================================================================
// Private helpers
// ===========================================================================

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

// ===========================================================================
// Subcommand handlers
// ===========================================================================

/// acs workflows list
pub async fn cmd_list(
    host: &str,
    port: u16,
    enabled: bool,
    disabled: bool,
    json: bool,
) -> anyhow::Result<()> {
    let client = Client::new();
    let mut url = format!("{}/api/workflows", base_url(host, port));

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

    let empty_vec = vec![];
    let workflows = body["workflows"].as_array().unwrap_or(&empty_vec);

    if workflows.is_empty() {
        println!("No workflows found.");
        return Ok(());
    }

    println!(
        "{:<16}{:<12}{:<20}{:<18}{:<18}",
        "NAME", "ENABLED", "SCHEDULE", "LAST RUN", "NEXT RUN"
    );

    for wf in workflows {
        let name = wf["name"].as_str().unwrap_or("?");
        let wf_enabled = wf["enabled"].as_bool().unwrap_or(false);
        let schedule = wf["schedule"].as_str().unwrap_or("?");

        let last_run = match wf["last_run_at"].as_str() {
            Some(ts) => {
                if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                    format_relative_time(&dt)
                } else {
                    "-".to_string()
                }
            }
            None => "-".to_string(),
        };

        let next_run = match wf["next_run_at"].as_str() {
            Some(ts) => {
                if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                    format_relative_time(&dt)
                } else {
                    "-".to_string()
                }
            }
            None => "-".to_string(),
        };

        let display_name = if name.len() > 15 {
            format!("{}...", &name[..12])
        } else {
            name.to_string()
        };

        let display_schedule = if schedule.len() > 19 {
            format!("{}...", &schedule[..16])
        } else {
            schedule.to_string()
        };

        println!(
            "{:<16}{:<12}{:<20}{:<18}{:<18}",
            display_name, wf_enabled, display_schedule, last_run, next_run
        );
    }

    Ok(())
}

/// acs workflows get <id-or-name>
pub async fn cmd_get(host: &str, port: u16, id_or_name: &str, json: bool) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!("{}/api/workflows/{}", base_url(host, port), id_or_name);

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

    // Human-readable key/value output
    println!("ID:          {}", body["id"].as_str().unwrap_or("?"));
    println!("Name:        {}", body["name"].as_str().unwrap_or("?"));
    println!("Version:     {}", body["version"].as_u64().unwrap_or(0));
    println!("Schedule:    {}", body["schedule"].as_str().unwrap_or("?"));
    if let Some(tz) = body["timezone"].as_str() {
        println!("Timezone:    {}", tz);
    }
    println!(
        "Enabled:     {}",
        body["enabled"].as_bool().unwrap_or(false)
    );

    let step_count = body["steps"].as_array().map(|a| a.len()).unwrap_or(0);
    println!("Steps:       {}", step_count);

    if let Some(ts) = body["last_run_at"].as_str() {
        if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
            println!("Last run:    {}", format_relative_time(&dt));
        }
    }
    if let Some(status_str) = body["last_run_status"].as_str() {
        println!("Last status: {}", status_str);
    }
    if let Some(ts) = body["next_run_at"].as_str() {
        if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
            println!("Next run:    {}", format_relative_time(&dt));
        }
    }
    println!(
        "Created:     {}",
        body["created_at"].as_str().unwrap_or("?")
    );
    println!(
        "Updated:     {}",
        body["updated_at"].as_str().unwrap_or("?")
    );

    Ok(())
}

/// acs workflows create
pub async fn cmd_create(
    host: &str,
    port: u16,
    file: Option<&str>,
    inline_json: Option<&str>,
) -> anyhow::Result<()> {
    let payload: Value = match (file, inline_json) {
        (Some(path), None) => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", path, e))?;
            serde_json::from_str(&content)
                .map_err(|e| anyhow::anyhow!("Invalid JSON in '{}': {}", path, e))?
        }
        (None, Some(json_str)) => serde_json::from_str(json_str)
            .map_err(|e| anyhow::anyhow!("Invalid inline JSON: {}", e))?,
        _ => {
            anyhow::bail!("Either --file or --json must be provided");
        }
    };

    let client = Client::new();
    let url = format!("{}/api/workflows", base_url(host, port));

    let response = client
        .post(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    if status.is_success() {
        let name = body["name"].as_str().unwrap_or("?");
        let id = body["id"].as_str().unwrap_or("?");
        println!("Workflow '{}' created successfully.", name);
        println!("  ID:      {}", id);
        println!("  Version: {}", body["version"].as_u64().unwrap_or(1));
        println!("  Enabled: {}", body["enabled"].as_bool().unwrap_or(true));
    } else {
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    Ok(())
}

/// acs workflows update <id-or-name>
pub async fn cmd_update(
    host: &str,
    port: u16,
    id_or_name: &str,
    file: Option<&str>,
    inline_json: Option<&str>,
    enable: bool,
    disable: bool,
) -> anyhow::Result<()> {
    let payload: Value = if let Some(path) = file {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("Failed to read file '{}': {}", path, e))?;
        serde_json::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Invalid JSON in '{}': {}", path, e))?
    } else if let Some(json_str) = inline_json {
        serde_json::from_str(json_str).map_err(|e| anyhow::anyhow!("Invalid inline JSON: {}", e))?
    } else if enable {
        serde_json::json!({"enabled": true})
    } else if disable {
        serde_json::json!({"enabled": false})
    } else {
        anyhow::bail!("Provide --file, --json, --enable, or --disable");
    };

    let client = Client::new();
    let url = format!("{}/api/workflows/{}", base_url(host, port), id_or_name);

    let response = client
        .patch(&url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to parse response: {}", e))?;

    if status.is_success() {
        let name = body["name"].as_str().unwrap_or(id_or_name);
        println!("Workflow '{}' updated.", name);
    } else {
        let message = body["message"].as_str().unwrap_or("Unknown error");
        eprintln!("Error: {}", message);
        std::process::exit(1);
    }

    Ok(())
}

/// acs workflows delete <id-or-name>
pub async fn cmd_delete(host: &str, port: u16, id_or_name: &str, yes: bool) -> anyhow::Result<()> {
    if !yes {
        print!(
            "Are you sure you want to delete workflow '{}'? [y/N] ",
            id_or_name
        );
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
    let url = format!("{}/api/workflows/{}", base_url(host, port), id_or_name);

    let response = client
        .delete(&url)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();

    if status.as_u16() == 204 {
        println!("Workflow '{}' deleted.", id_or_name);
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

/// acs workflows trigger <id-or-name>
pub async fn cmd_trigger(
    host: &str,
    port: u16,
    id_or_name: &str,
    input: Option<&str>,
    input_file: Option<&str>,
    env: &[String],
    target_step: Option<&str>,
    follow: bool,
) -> anyhow::Result<()> {
    // Parse input JSON
    let input_value: Option<Value> = match (input, input_file) {
        (Some(json_str), None) => Some(
            serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("Invalid input JSON: {}", e))?,
        ),
        (None, Some(path)) => {
            let content = std::fs::read_to_string(path)
                .map_err(|e| anyhow::anyhow!("Failed to read input file '{}': {}", path, e))?;
            Some(
                serde_json::from_str(&content)
                    .map_err(|e| anyhow::anyhow!("Invalid JSON in input file '{}': {}", path, e))?,
            )
        }
        (None, None) => None,
        _ => unreachable!("clap conflicts_with ensures only one of --input/--input-file"),
    };

    // Parse env vars
    let env_map = if env.is_empty() {
        None
    } else {
        let parsed = parse_env_vars(env).map_err(|e| anyhow::anyhow!(e))?;
        Some(parsed)
    };

    // Build TriggerParams body
    let mut body_map = serde_json::Map::new();
    body_map.insert("input".to_string(), input_value.unwrap_or(Value::Null));
    if let Some(env_map) = env_map {
        body_map.insert("env".to_string(), serde_json::to_value(env_map)?);
    }
    if let Some(ts) = target_step {
        body_map.insert("target_step".to_string(), Value::String(ts.to_string()));
    }
    let trigger_body = Value::Object(body_map);

    let client = Client::new();

    if follow {
        // Establish SSE stream BEFORE triggering to avoid race condition
        // 1. Resolve workflow id for SSE filter
        let workflow_id = resolve_workflow_id(&client, host, port, id_or_name).await?;

        // 2. Open SSE connection on workflow_id filter first, then filter by run_id once known
        let sse_url = format!(
            "{}/api/events/workflows?workflow_id={}",
            base_url(host, port),
            workflow_id
        );
        let sse_response = client
            .get(&sse_url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to connect to SSE stream: {}", e))?;

        // 3. Trigger
        let trigger_url = format!(
            "{}/api/workflows/{}/trigger",
            base_url(host, port),
            id_or_name
        );
        let response = client
            .post(&trigger_url)
            .json(&trigger_body)
            .send()
            .await
            .map_err(|e| handle_request_error(e, host, port))?;

        let status = response.status();
        let resp_body: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse trigger response: {}", e))?;

        if !status.is_success() {
            let message = resp_body["message"].as_str().unwrap_or("Unknown error");
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }

        let run_id = resp_body["run_id"].as_str().unwrap_or("unknown");
        let wf_id_str = resp_body["workflow_id"].as_str().unwrap_or(id_or_name);
        println!("Workflow '{}' triggered (run: {}).", wf_id_str, run_id);
        println!("Following events (run_id={})...", run_id);

        // 4. Stream SSE events, filtering by run_id in the event data
        follow_workflow_sse_stream(sse_response, run_id).await?;
    } else {
        let trigger_url = format!(
            "{}/api/workflows/{}/trigger",
            base_url(host, port),
            id_or_name
        );
        let response = client
            .post(&trigger_url)
            .json(&trigger_body)
            .send()
            .await
            .map_err(|e| handle_request_error(e, host, port))?;

        let status = response.status();
        let resp_body: Value = response
            .json()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to parse trigger response: {}", e))?;

        if !status.is_success() {
            let message = resp_body["message"].as_str().unwrap_or("Unknown error");
            eprintln!("Error: {}", message);
            std::process::exit(1);
        }

        let run_id = resp_body["run_id"].as_str().unwrap_or("unknown");
        let wf_id_str = resp_body["workflow_id"].as_str().unwrap_or(id_or_name);
        println!("Workflow '{}' triggered (run: {}).", wf_id_str, run_id);
    }

    Ok(())
}

/// acs workflows runs <name-or-id>
pub async fn cmd_runs(
    host: &str,
    port: u16,
    name_or_id: &str,
    limit: usize,
    offset: usize,
    json: bool,
) -> anyhow::Result<()> {
    let client = Client::new();
    let url = format!(
        "{}/api/workflows/{}/runs?limit={}&offset={}",
        base_url(host, port),
        name_or_id,
        limit,
        offset
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| handle_request_error(e, host, port))?;

    let status = response.status();

    if status.as_u16() == 404 {
        anyhow::bail!("Workflow not found: {}", name_or_id);
    }

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

    let total = body["total"].as_u64().unwrap_or(0);
    println!("Total runs: {}", total);

    let empty_vec = vec![];
    let runs = body["runs"].as_array().unwrap_or(&empty_vec);

    if runs.is_empty() {
        println!("No runs found.");
        return Ok(());
    }

    println!(
        "{:<38}{:<12}{:<28}{:<14}{:<12}",
        "RUN_ID", "STATUS", "STARTED", "DURATION_MS", "COST_USD"
    );

    for run in runs {
        let run_id = run["run_id"].as_str().unwrap_or("?");
        let run_status = run["status"].as_str().unwrap_or("?");

        let started = match run["started_at"].as_str() {
            Some(ts) => {
                if let Ok(dt) = ts.parse::<DateTime<Utc>>() {
                    format_relative_time(&dt)
                } else {
                    ts.to_string()
                }
            }
            None => "-".to_string(),
        };

        let duration = match run["total_duration_ms"].as_u64() {
            Some(ms) => ms.to_string(),
            None => "-".to_string(),
        };

        let cost = match run["total_cost_usd"].as_f64() {
            Some(c) => format!("{:.6}", c),
            None => "-".to_string(),
        };

        println!(
            "{:<38}{:<12}{:<28}{:<14}{:<12}",
            run_id, run_status, started, duration, cost
        );
    }

    Ok(())
}

/// Resolve a workflow name or UUID to its workflow_id string.
async fn resolve_workflow_id(
    client: &Client,
    host: &str,
    port: u16,
    id_or_name: &str,
) -> anyhow::Result<String> {
    let url = format!("{}/api/workflows/{}", base_url(host, port), id_or_name);

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
        let message = body["message"].as_str().unwrap_or("Workflow not found");
        anyhow::bail!("Error: {}", message);
    }

    let id = body["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing workflow ID in response"))?;

    Ok(id.to_string())
}

/// Follow a workflow SSE stream from an already-connected response.
///
/// The SSE events for workflows use these event type names (from sse.rs):
///   run_started, step_started, step_output, step_completed, run_completed, run_failed
///
/// We exit on `run_completed` or `run_failed` events matching the given run_id.
/// If run_id is provided we filter data-level run_id; if it doesn't match we continue.
async fn follow_workflow_sse_stream(
    response: reqwest::Response,
    run_id: &str,
) -> anyhow::Result<()> {
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

            let mut event_type = String::new();
            let mut data = String::new();

            for line in event_block.lines() {
                if let Some(rest) = line.strip_prefix("event: ") {
                    event_type = rest.to_string();
                } else if let Some(rest) = line.strip_prefix("data: ") {
                    if !data.is_empty() {
                        data.push('\n');
                    }
                    data.push_str(rest);
                }
            }

            // Parse data JSON to check run_id
            let json: Option<Value> = serde_json::from_str(&data).ok();

            // Helper: extract run_id from parsed event JSON.
            // WorkflowEvent variants serialize with their field names directly.
            let event_run_id = json
                .as_ref()
                .and_then(|v| v.get("run_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            // Skip if this event is for a different run
            if let Some(ref eid) = event_run_id {
                if eid != run_id {
                    continue;
                }
            }

            match event_type.as_str() {
                "run_started" => {
                    println!("--- Run started ---");
                }
                "step_started" => {
                    if let Some(ref v) = json {
                        let step_id = v["step_id"].as_str().unwrap_or("?");
                        println!("  Step started: {}", step_id);
                    }
                }
                "step_output" => {
                    if let Some(ref v) = json {
                        // StepOutput carries "output" field
                        if let Some(output) = v["output"].as_str() {
                            print!("{}", output);
                            io::stdout().flush()?;
                        }
                    }
                }
                "step_completed" => {
                    if let Some(ref v) = json {
                        let step_id = v["step_id"].as_str().unwrap_or("?");
                        let status = v["status"].as_str().unwrap_or("?");
                        println!("  Step completed: {} ({})", step_id, status);
                    }
                }
                "run_completed" => {
                    println!("\n--- Run completed ---");
                    return Ok(());
                }
                "run_failed" => {
                    if let Some(ref v) = json {
                        if let Some(error) = v["error"].as_str() {
                            eprintln!("\n--- Run failed: {} ---", error);
                        } else {
                            eprintln!("\n--- Run failed ---");
                        }
                    } else {
                        eprintln!("\n--- Run failed ---");
                    }
                    return Ok(());
                }
                "keepalive" | "" => {
                    // Ignore
                }
                _ => {
                    // workflow_changed and others — ignore silently
                }
            }
        }
    }

    Ok(())
}

// ===========================================================================
// Top-level dispatcher
// ===========================================================================

/// Dispatch a WorkflowsCmd to the appropriate handler.
pub async fn dispatch(cmd: &WorkflowsCmd, host: &str, port: u16) -> anyhow::Result<()> {
    match &cmd.subcommand {
        WorkflowsSubcommand::List {
            enabled,
            disabled,
            json,
        } => cmd_list(host, port, *enabled, *disabled, *json).await,

        WorkflowsSubcommand::Get { id_or_name, json } => {
            cmd_get(host, port, id_or_name, *json).await
        }

        WorkflowsSubcommand::Create { file, inline_json } => {
            cmd_create(host, port, file.as_deref(), inline_json.as_deref()).await
        }

        WorkflowsSubcommand::Update {
            id_or_name,
            file,
            inline_json,
            enable,
            disable,
        } => {
            cmd_update(
                host,
                port,
                id_or_name,
                file.as_deref(),
                inline_json.as_deref(),
                *enable,
                *disable,
            )
            .await
        }

        WorkflowsSubcommand::Delete { id_or_name, yes } => {
            cmd_delete(host, port, id_or_name, *yes).await
        }

        WorkflowsSubcommand::Trigger {
            id_or_name,
            input,
            input_file,
            env,
            target_step,
            follow,
        } => {
            cmd_trigger(
                host,
                port,
                id_or_name,
                input.as_deref(),
                input_file.as_deref(),
                env,
                target_step.as_deref(),
                *follow,
            )
            .await
        }

        WorkflowsSubcommand::Runs {
            name_or_id,
            limit,
            offset,
            json,
        } => cmd_runs(host, port, name_or_id, *limit, *offset, *json).await,
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::{parse_env_vars, Cli};
    use clap::Parser;

    // -----------------------------------------------------------------------
    // Argument parsing: workflows list
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_list_parses() {
        let cli =
            Cli::try_parse_from(["acs", "workflows", "list"]).expect("Should parse workflows list");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => {
                assert!(matches!(
                    cmd.subcommand,
                    WorkflowsSubcommand::List {
                        enabled: false,
                        disabled: false,
                        json: false
                    }
                ));
            }
            other => panic!("Expected Workflows command, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_list_enabled_flag() {
        let cli = Cli::try_parse_from(["acs", "workflows", "list", "--enabled"])
            .expect("Should parse workflows list --enabled");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::List { enabled, .. } => {
                    assert!(enabled);
                }
                other => panic!("Expected List, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_list_disabled_flag() {
        let cli = Cli::try_parse_from(["acs", "workflows", "list", "--disabled"])
            .expect("Should parse workflows list --disabled");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::List { disabled, .. } => {
                    assert!(disabled);
                }
                other => panic!("Expected List, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_list_enabled_disabled_conflict() {
        let result = Cli::try_parse_from(["acs", "workflows", "list", "--enabled", "--disabled"]);
        assert!(result.is_err(), "--enabled and --disabled should conflict");
    }

    #[test]
    fn test_workflows_list_json_flag() {
        let cli = Cli::try_parse_from(["acs", "workflows", "list", "--json"])
            .expect("Should parse workflows list --json");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::List { json, .. } => {
                    assert!(json);
                }
                other => panic!("Expected List, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Argument parsing: workflows get
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_get_parses() {
        let cli = Cli::try_parse_from(["acs", "workflows", "get", "my-workflow"])
            .expect("Should parse workflows get");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Get { id_or_name, json } => {
                    assert_eq!(id_or_name, "my-workflow");
                    assert!(!json);
                }
                other => panic!("Expected Get, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_get_json_flag() {
        let cli = Cli::try_parse_from(["acs", "workflows", "get", "my-workflow", "--json"])
            .expect("Should parse workflows get --json");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Get { json, .. } => {
                    assert!(json);
                }
                other => panic!("Expected Get, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Argument parsing: workflows create
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_create_with_file() {
        let cli = Cli::try_parse_from(["acs", "workflows", "create", "--file", "/tmp/wf.json"])
            .expect("Should parse workflows create --file");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Create { file, inline_json } => {
                    assert_eq!(file.as_deref(), Some("/tmp/wf.json"));
                    assert!(inline_json.is_none());
                }
                other => panic!("Expected Create, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_create_with_inline_json() {
        let cli =
            Cli::try_parse_from(["acs", "workflows", "create", "--json", r#"{"name":"test"}"#])
                .expect("Should parse workflows create --json");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Create { file, inline_json } => {
                    assert!(file.is_none());
                    assert_eq!(inline_json.as_deref(), Some(r#"{"name":"test"}"#));
                }
                other => panic!("Expected Create, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_create_file_and_json_conflict() {
        let result = Cli::try_parse_from([
            "acs",
            "workflows",
            "create",
            "--file",
            "/tmp/wf.json",
            "--json",
            r#"{"name":"test"}"#,
        ]);
        assert!(result.is_err(), "--file and --json should conflict");
    }

    // -----------------------------------------------------------------------
    // Argument parsing: workflows update
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_update_with_enable() {
        let cli = Cli::try_parse_from(["acs", "workflows", "update", "my-workflow", "--enable"])
            .expect("Should parse workflows update --enable");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Update {
                    id_or_name,
                    enable,
                    disable,
                    ..
                } => {
                    assert_eq!(id_or_name, "my-workflow");
                    assert!(enable);
                    assert!(!disable);
                }
                other => panic!("Expected Update, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_update_with_disable() {
        let cli = Cli::try_parse_from(["acs", "workflows", "update", "my-workflow", "--disable"])
            .expect("Should parse workflows update --disable");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Update { disable, .. } => {
                    assert!(disable);
                }
                other => panic!("Expected Update, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_update_enable_disable_conflict() {
        let result = Cli::try_parse_from([
            "acs",
            "workflows",
            "update",
            "my-workflow",
            "--enable",
            "--disable",
        ]);
        assert!(result.is_err(), "--enable and --disable should conflict");
    }

    #[test]
    fn test_workflows_update_with_file() {
        let cli = Cli::try_parse_from([
            "acs",
            "workflows",
            "update",
            "my-workflow",
            "--file",
            "/tmp/update.json",
        ])
        .expect("Should parse workflows update --file");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Update { file, .. } => {
                    assert_eq!(file.as_deref(), Some("/tmp/update.json"));
                }
                other => panic!("Expected Update, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_update_file_and_json_conflict() {
        let result = Cli::try_parse_from([
            "acs",
            "workflows",
            "update",
            "my-workflow",
            "--file",
            "/tmp/update.json",
            "--json",
            r#"{"enabled":true}"#,
        ]);
        assert!(result.is_err(), "--file and --json should conflict");
    }

    // -----------------------------------------------------------------------
    // Argument parsing: workflows delete
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_delete_parses() {
        let cli = Cli::try_parse_from(["acs", "workflows", "delete", "my-workflow"])
            .expect("Should parse workflows delete");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Delete { id_or_name, yes } => {
                    assert_eq!(id_or_name, "my-workflow");
                    assert!(!yes);
                }
                other => panic!("Expected Delete, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_delete_yes_flag() {
        let cli = Cli::try_parse_from(["acs", "workflows", "delete", "my-workflow", "-y"])
            .expect("Should parse workflows delete -y");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Delete { yes, .. } => {
                    assert!(yes);
                }
                other => panic!("Expected Delete, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // Argument parsing: workflows trigger
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_trigger_basic() {
        let cli = Cli::try_parse_from(["acs", "workflows", "trigger", "my-workflow"])
            .expect("Should parse workflows trigger");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Trigger {
                    id_or_name,
                    input,
                    input_file,
                    env,
                    target_step,
                    follow,
                } => {
                    assert_eq!(id_or_name, "my-workflow");
                    assert!(input.is_none());
                    assert!(input_file.is_none());
                    assert!(env.is_empty());
                    assert!(target_step.is_none());
                    assert!(!follow);
                }
                other => panic!("Expected Trigger, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_trigger_with_input() {
        let cli = Cli::try_parse_from([
            "acs",
            "workflows",
            "trigger",
            "my-workflow",
            "--input",
            r#"{"key":"value"}"#,
        ])
        .expect("Should parse workflows trigger --input");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Trigger { input, .. } => {
                    assert_eq!(input.as_deref(), Some(r#"{"key":"value"}"#));
                }
                other => panic!("Expected Trigger, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_trigger_with_env() {
        let cli = Cli::try_parse_from([
            "acs",
            "workflows",
            "trigger",
            "my-workflow",
            "--env",
            "FOO=bar",
            "--env",
            "BAZ=qux",
        ])
        .expect("Should parse workflows trigger --env");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Trigger { env, .. } => {
                    assert_eq!(env.len(), 2);
                    assert_eq!(env[0], "FOO=bar");
                    assert_eq!(env[1], "BAZ=qux");
                }
                other => panic!("Expected Trigger, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_trigger_with_target_step() {
        let cli = Cli::try_parse_from([
            "acs",
            "workflows",
            "trigger",
            "my-workflow",
            "--target-step",
            "step-2",
        ])
        .expect("Should parse workflows trigger --target-step");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Trigger { target_step, .. } => {
                    assert_eq!(target_step.as_deref(), Some("step-2"));
                }
                other => panic!("Expected Trigger, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_trigger_follow_flag() {
        let cli = Cli::try_parse_from(["acs", "workflows", "trigger", "my-workflow", "--follow"])
            .expect("Should parse workflows trigger --follow");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Trigger { follow, .. } => {
                    assert!(follow);
                }
                other => panic!("Expected Trigger, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_workflows_trigger_input_and_input_file_conflict() {
        let result = Cli::try_parse_from([
            "acs",
            "workflows",
            "trigger",
            "my-workflow",
            "--input",
            r#"{"a":1}"#,
            "--input-file",
            "/tmp/input.json",
        ]);
        assert!(result.is_err(), "--input and --input-file should conflict");
    }

    #[test]
    fn test_workflows_trigger_with_input_and_env() {
        let cli = Cli::try_parse_from([
            "acs",
            "workflows",
            "trigger",
            "my-workflow",
            "--input",
            r#"{"prompt":"hello"}"#,
            "--env",
            "MODEL=claude-3-5",
        ])
        .expect("Should parse workflows trigger with input and env");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Trigger { input, env, .. } => {
                    assert_eq!(input.as_deref(), Some(r#"{"prompt":"hello"}"#));
                    assert_eq!(env.len(), 1);
                    assert_eq!(env[0], "MODEL=claude-3-5");
                }
                other => panic!("Expected Trigger, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    // -----------------------------------------------------------------------
    // parse_env_vars reuse
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflows_env_arg_parsing_valid() {
        let args = vec!["KEY=value".to_string(), "FOO=bar=baz".to_string()];
        let result = parse_env_vars(&args).unwrap();
        assert_eq!(result.get("KEY"), Some(&"value".to_string()));
        assert_eq!(result.get("FOO"), Some(&"bar=baz".to_string()));
    }

    #[test]
    fn test_workflows_env_arg_parsing_no_equals_rejected() {
        let args = vec!["NOEQUALS".to_string()];
        let result = parse_env_vars(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_workflows_env_arg_parsing_empty_key_rejected() {
        let args = vec!["=value".to_string()];
        let result = parse_env_vars(&args);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // Format helpers
    // -----------------------------------------------------------------------

    #[test]
    fn test_format_relative_time_past_minutes() {
        let past = Utc::now() - chrono::Duration::minutes(7);
        let result = format_relative_time(&past);
        assert!(result.contains("minutes ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_future_hours() {
        let future = Utc::now() + chrono::Duration::hours(3);
        let result = format_relative_time(&future);
        assert!(
            result.contains("in") && result.contains("hours"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_format_relative_time_past_seconds() {
        let past = Utc::now() - chrono::Duration::seconds(10);
        let result = format_relative_time(&past);
        assert!(result.contains("seconds ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_past_days() {
        let past = Utc::now() - chrono::Duration::days(5);
        let result = format_relative_time(&past);
        assert!(result.contains("days ago"), "Got: {}", result);
    }

    #[test]
    fn test_format_relative_time_in_minutes() {
        let future = Utc::now() + chrono::Duration::minutes(20);
        let result = format_relative_time(&future);
        assert!(
            result.contains("in") && result.contains("minutes"),
            "Got: {}",
            result
        );
    }

    #[test]
    fn test_format_relative_time_in_days() {
        let future = Utc::now() + chrono::Duration::days(3);
        let result = format_relative_time(&future);
        assert!(
            result.contains("in") && result.contains("days"),
            "Got: {}",
            result
        );
    }

    // -----------------------------------------------------------------------
    // SSE data parsing (mirrors jobs.rs SSE tests)
    // -----------------------------------------------------------------------

    #[test]
    fn test_workflow_sse_event_block_parsing() {
        let event_block =
            "event: run_completed\ndata: {\"run_id\":\"abc\",\"workflow_id\":\"def\"}\n";
        let mut event_type = String::new();
        let mut data = String::new();
        for line in event_block.lines() {
            if let Some(rest) = line.strip_prefix("event: ") {
                event_type = rest.to_string();
            } else if let Some(rest) = line.strip_prefix("data: ") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest);
            }
        }
        assert_eq!(event_type, "run_completed");
        assert_eq!(data, r#"{"run_id":"abc","workflow_id":"def"}"#);
    }

    #[test]
    fn test_workflow_sse_step_output_parsing() {
        let event_block =
            "event: step_output\ndata: {\"run_id\":\"abc\",\"output\":\"hello world\"}\n";
        let mut data = String::new();
        for line in event_block.lines() {
            if let Some(rest) = line.strip_prefix("data: ") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(rest);
            }
        }
        let json: Value = serde_json::from_str(&data).unwrap();
        assert_eq!(json["output"].as_str().unwrap(), "hello world");
    }

    // -----------------------------------------------------------------------
    // Argument parsing: workflows runs
    // -----------------------------------------------------------------------

    #[test]
    fn test_cli_workflows_runs_parses() {
        let cli = Cli::try_parse_from([
            "acs",
            "workflows",
            "runs",
            "my-wf",
            "--limit",
            "10",
            "--offset",
            "5",
            "--json",
        ])
        .expect("Should parse workflows runs with all flags");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Runs {
                    name_or_id,
                    limit,
                    offset,
                    json,
                } => {
                    assert_eq!(name_or_id, "my-wf");
                    assert_eq!(*limit, 10);
                    assert_eq!(*offset, 5);
                    assert!(json);
                }
                other => panic!("Expected Runs, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }

    #[test]
    fn test_cli_workflows_runs_default_limit_offset() {
        let cli = Cli::try_parse_from(["acs", "workflows", "runs", "my-wf"])
            .expect("Should parse workflows runs with no flags");
        match &cli.command {
            Some(crate::cli::Commands::Workflows(cmd)) => match &cmd.subcommand {
                WorkflowsSubcommand::Runs {
                    name_or_id,
                    limit,
                    offset,
                    json,
                } => {
                    assert_eq!(name_or_id, "my-wf");
                    assert_eq!(*limit, 20);
                    assert_eq!(*offset, 0);
                    assert!(!json);
                }
                other => panic!("Expected Runs, got: {:?}", other),
            },
            other => panic!("Expected Workflows, got: {:?}", other),
        }
    }
}
