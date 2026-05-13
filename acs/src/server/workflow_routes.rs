//! HTTP handlers for the /api/workflows and /api/runs endpoints.
//!
//! Mounted by `server/mod.rs`:
//!   GET    /api/workflows
//!   POST   /api/workflows
//!   GET    /api/workflows/{id}
//!   PATCH  /api/workflows/{id}
//!   DELETE /api/workflows/{id}
//!   POST   /api/workflows/{id}/trigger
//!   GET    /api/runs/{run_id}
//!   POST   /api/runs/{run_id}/kill
//!
//! The `{id}` path parameter accepts both a UUID and a workflow name (slug).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use chrono_tz::Tz;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::daemon::events::{WorkflowChangeKind, WorkflowEvent};
use crate::errors::AcsError;
use crate::models::workflow::{
    CostSummary, CostWorkflowResponse, CostWorkflowsListResponse, NewWorkflow, RunStatus,
    TriggerParams, Workflow, WorkflowCostEntry, WorkflowRun, WorkflowUpdate,
};
use crate::workflow::{EventEmittingLogSink, FileLogSink};

// ---------------------------------------------------------------------------
// AgentStep.command_template guard
// ---------------------------------------------------------------------------

/// Walk a JSON steps array and return `true` if any step has `kind: "agent"`
/// and a `command_template` key. Used by POST and PATCH handlers to reject
/// payloads that reference the removed field.
fn steps_contain_command_template(steps: &serde_json::Value) -> bool {
    let arr = match steps.as_array() {
        Some(a) => a,
        None => return false,
    };
    for step in arr {
        let obj = match step.as_object() {
            Some(o) => o,
            None => continue,
        };
        if obj.get("kind").and_then(|v| v.as_str()) == Some("agent")
            && obj.contains_key("command_template")
        {
            return true;
        }
        // Recurse into MatchStep branches
        if let Some(cases) = obj.get("cases").and_then(|v| v.as_object()) {
            for branch in cases.values() {
                if steps_contain_command_template(branch) {
                    return true;
                }
            }
        }
        if let Some(default_steps) = obj.get("default") {
            if steps_contain_command_template(default_steps) {
                return true;
            }
        }
    }
    false
}

const COMMAND_TEMPLATE_REMOVED_ERROR: &str = "command_template_removed";
const COMMAND_TEMPLATE_REMOVED_MESSAGE: &str =
    "AgentStep.command_template was removed in v4.2.7. Use 'model' and 'extra_args' fields instead. See docs/workflow-management.md.";

// ---------------------------------------------------------------------------
// Error response
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
}

fn error_response(status: StatusCode, error: &str, message: &str) -> impl IntoResponse {
    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
            message: message.to_string(),
        }),
    )
}

/// Map an `anyhow::Error` (wrapping AcsError) to the appropriate HTTP response.
fn map_store_error(e: anyhow::Error) -> (StatusCode, Json<ErrorResponse>) {
    // Downcast to AcsError when possible for precise status codes.
    if let Some(acs) = e.downcast_ref::<AcsError>() {
        match acs {
            AcsError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: msg.clone(),
                }),
            ),
            AcsError::Conflict(msg) => (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "conflict".to_string(),
                    message: msg.clone(),
                }),
            ),
            // Both Validation and Cron errors are user-facing input problems
            // that map to 422 Unprocessable Entity.
            AcsError::Validation(msg) | AcsError::Cron(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                Json(ErrorResponse {
                    error: "validation_error".to_string(),
                    message: msg.clone(),
                }),
            ),
            _ => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: e.to_string(),
                }),
            ),
        }
    } else {
        // Check string-based error messages for conflict/not-found patterns
        let msg = e.to_string();
        if msg.contains("Conflict") || msg.contains("already exists") {
            (
                StatusCode::CONFLICT,
                Json(ErrorResponse {
                    error: "conflict".to_string(),
                    message: msg,
                }),
            )
        } else if msg.contains("not found") || msg.contains("Not found") {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: msg,
                }),
            )
        } else {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: msg,
                }),
            )
        }
    }
}

// ---------------------------------------------------------------------------
// Workflow ID resolution: try UUID first, then name lookup
// ---------------------------------------------------------------------------

async fn resolve_workflow(
    state: &AppState,
    id_or_name: &str,
) -> Result<Workflow, (StatusCode, Json<ErrorResponse>)> {
    // Try UUID parse first
    if let Ok(uuid) = Uuid::parse_str(id_or_name) {
        match state.workflow_store.get_workflow(uuid).await {
            Ok(Some(wf)) => return Ok(wf),
            Ok(None) => {
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "not_found".to_string(),
                        message: format!("Workflow with id '{}' not found", id_or_name),
                    }),
                ));
            }
            Err(e) => {
                tracing::warn!("Failed to fetch workflow '{}': {}", id_or_name, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to fetch workflow: {}", e),
                    }),
                ));
            }
        }
    }

    // Not a valid UUID — try name lookup
    match state.workflow_store.find_by_name(id_or_name).await {
        Ok(Some(wf)) => Ok(wf),
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse {
                error: "not_found".to_string(),
                message: format!("Workflow with name '{}' not found", id_or_name),
            }),
        )),
        Err(e) => {
            tracing::warn!("Failed to find workflow by name '{}': {}", id_or_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch workflow: {}", e),
                }),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Cost window query params + validation
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct CostWindowParams {
    pub days: Option<u32>,
    pub since: Option<String>,
    pub until: Option<String>,
}

/// Parse an ISO-8601 date string ("YYYY-MM-DD") into a `NaiveDate`.
fn parse_iso_date(s: &str) -> Result<NaiveDate, (StatusCode, Json<ErrorResponse>)> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: "invalid_window".to_string(),
                message: format!("Cannot parse date '{}'; expected YYYY-MM-DD", s),
            }),
        )
    })
}

/// Resolve the cost window from the query params.
///
/// Returns `(since_utc, until_utc)` — both are midnight boundaries in
/// `display_tz`, converted to UTC.
///
/// Validation rules:
/// - `days` AND (`since` OR `until`) together → 400 `conflicting_params`
/// - `days > 365` → 400 `window_too_large`
/// - Only one of `since`/`until` provided → 400 `invalid_window`
/// - `since >= until` → 400 `invalid_window`
/// - span > 365 days → 400 `window_too_large`
/// - Default (no params) → last 30 days
#[allow(clippy::type_complexity)]
fn resolve_window(
    p: &CostWindowParams,
    display_tz: Tz,
) -> Result<(DateTime<Utc>, DateTime<Utc>), (StatusCode, Json<ErrorResponse>)> {
    let bad = |code: &str, msg: &str| -> (StatusCode, Json<ErrorResponse>) {
        (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse {
                error: code.to_string(),
                message: msg.to_string(),
            }),
        )
    };

    // Mutex: days + (since | until) is invalid.
    if p.days.is_some() && (p.since.is_some() || p.until.is_some()) {
        return Err(bad(
            "conflicting_params",
            "use either `days` OR `since`+`until`, not both",
        ));
    }

    let now_utc = Utc::now();
    let now_local = now_utc.with_timezone(&display_tz);
    let today_naive = now_local.date_naive();

    // `until` = start of tomorrow in display_tz (exclusive upper bound = end of today).
    let tomorrow_naive = today_naive
        .succ_opt()
        .expect("date arithmetic should not overflow");
    let until_midnight = display_tz
        .from_local_datetime(
            &tomorrow_naive
                .and_hms_opt(0, 0, 0)
                .expect("00:00:00 is always valid"),
        )
        .earliest()
        .unwrap_or_else(|| {
            display_tz
                .from_local_datetime(
                    &tomorrow_naive
                        .and_hms_opt(0, 0, 0)
                        .expect("00:00:00 is always valid"),
                )
                .latest()
                .expect("at least one mapping must exist")
        })
        .with_timezone(&Utc);

    let (since, until) = if let Some(d) = p.days {
        if d == 0 {
            return Err(bad("invalid_window", "days must be >= 1"));
        }
        if d > 365 {
            return Err(bad("window_too_large", "max window is 365 days"));
        }
        let until = until_midnight;
        let since = until - Duration::days(d as i64);
        (since, until)
    } else if p.since.is_some() && p.until.is_some() {
        let since_date = parse_iso_date(p.since.as_deref().unwrap())?;
        let until_date = parse_iso_date(p.until.as_deref().unwrap())?;
        if since_date >= until_date {
            return Err(bad("invalid_window", "`since` must be before `until`"));
        }
        let span_days = (until_date - since_date).num_days();
        if span_days > 365 {
            return Err(bad("window_too_large", "max window is 365 days"));
        }
        let to_utc = |d: NaiveDate| {
            display_tz
                .from_local_datetime(&d.and_hms_opt(0, 0, 0).expect("00:00:00 is always valid"))
                .earliest()
                .unwrap_or_else(|| {
                    display_tz
                        .from_local_datetime(
                            &d.and_hms_opt(0, 0, 0).expect("00:00:00 is always valid"),
                        )
                        .latest()
                        .expect("at least one mapping must exist")
                })
                .with_timezone(&Utc)
        };
        (to_utc(since_date), to_utc(until_date))
    } else if p.since.is_some() || p.until.is_some() {
        return Err(bad(
            "invalid_window",
            "both `since` and `until` are required when not using `days`",
        ));
    } else {
        // Default: last 30 days.
        let until = until_midnight;
        let since = until - Duration::days(30);
        (since, until)
    };

    Ok((since, until))
}

// ---------------------------------------------------------------------------
// GET /api/workflows
// ---------------------------------------------------------------------------

pub async fn list_workflows(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    match state.workflow_store.list_workflows().await {
        Ok(workflows) => (
            StatusCode::OK,
            Json(serde_json::to_value(workflows).unwrap()),
        )
            .into_response(),
        Err(e) => {
            tracing::error!("Failed to list workflows: {}", e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            )
            .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/workflows
// ---------------------------------------------------------------------------

/// Error returned by [`apply_new_workflow_defaults`] when it cannot populate a
/// missing `working_dir`. Surfaced by [`create_workflow`] as a 422 response so
/// the caller knows the workflow was NOT persisted (instead of being saved with
/// a silent `working_dir = null`).
struct DefaultWorkingDirError {
    path: std::path::PathBuf,
    io_err: std::io::Error,
}

/// Fill in daemon-config-driven defaults on a freshly-deserialised
/// `NewWorkflow` body. Called from `create_workflow` before the body reaches
/// the store.
///
/// Currently fills in:
///   * `timezone` ← `config.display_timezone`
///   * `working_dir` ← `<root>/<sanitized-name>/`, where `<root>` is
///     `config.display_workflow_dir_root` when set, otherwise
///     `<documents>/agent-cron-scheduler`. The directory is created on disk;
///     on `create_dir_all` failure this function returns
///     `Err(DefaultWorkingDirError)` so the caller can reject the request with
///     a 422 instead of persisting a workflow with `working_dir = null`.
fn apply_new_workflow_defaults(
    body: &mut NewWorkflow,
    config: &crate::models::DaemonConfig,
) -> Result<(), DefaultWorkingDirError> {
    if body.timezone.is_none() {
        body.timezone = Some(config.display_timezone.clone());
    }

    if body.working_dir.is_none() {
        let root: Option<std::path::PathBuf> = match config.display_workflow_dir_root.as_deref() {
            Some(p) => Some(p.to_path_buf()),
            None => dirs::document_dir().map(|d| d.join("agent-cron-scheduler")),
        };

        match root {
            Some(root_path) => {
                let sanitized = sanitize_workflow_name(&body.name);
                let target = root_path.join(&sanitized);
                match std::fs::create_dir_all(&target) {
                    Ok(()) => {
                        body.working_dir = Some(target.to_string_lossy().into_owned());
                    }
                    Err(e) => {
                        return Err(DefaultWorkingDirError {
                            path: target,
                            io_err: e,
                        });
                    }
                }
            }
            None => {
                tracing::warn!(
                    workflow_name = %body.name,
                    "could not resolve user Documents dir and no \
                     display_workflow_dir_root configured — persisting workflow \
                     with working_dir = null"
                );
            }
        }
    }
    Ok(())
}

/// Sanitize a workflow name for use as a directory component.
///
/// Lowercases, keeps `[a-z0-9-]`, and replaces every other run of characters
/// with a single `-`. Leading and trailing dashes are stripped. If the input
/// reduces to the empty string, the literal `workflow` is returned so the
/// path component is always non-empty.
fn sanitize_workflow_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lowered = ch.to_ascii_lowercase();
        if lowered.is_ascii_alphanumeric() {
            out.push(lowered);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "workflow".to_string()
    } else {
        trimmed
    }
}

pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(raw): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Reject payloads that reference the removed command_template field before
    // deserialising into the typed struct, so callers get a clear error message.
    if let Some(steps) = raw.get("steps") {
        if steps_contain_command_template(steps) {
            return error_response(
                StatusCode::BAD_REQUEST,
                COMMAND_TEMPLATE_REMOVED_ERROR,
                COMMAND_TEMPLATE_REMOVED_MESSAGE,
            )
            .into_response();
        }
    }

    let mut body: NewWorkflow = match serde_json::from_value(raw) {
        Ok(b) => b,
        Err(e) => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "deserialization_error",
                &e.to_string(),
            )
            .into_response();
        }
    };

    // Apply daemon-config defaults for fields the caller left unspecified.
    if let Err(e) = apply_new_workflow_defaults(&mut body, &state.config) {
        return error_response(
            StatusCode::UNPROCESSABLE_ENTITY,
            "working_dir_create_failed",
            &format!(
                "Could not create default working_dir at {}: {}. Specify working_dir explicitly.",
                e.path.display(),
                e.io_err
            ),
        )
        .into_response();
    }

    match state.workflow_store.create_workflow(body).await {
        Ok(wf) => {
            // Broadcast WorkflowChanged{Created}
            let _ = state
                .workflow_event_tx
                .send(WorkflowEvent::WorkflowChanged {
                    workflow_id: wf.id,
                    version: wf.version,
                    change_kind: WorkflowChangeKind::Created,
                });
            tracing::info!(workflow_id = %wf.id, name = %wf.name, "workflow created");
            (
                StatusCode::CREATED,
                Json(serde_json::to_value(&wf).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            let (status, body) = map_store_error(e);
            (status, body).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/workflows/{id}
// ---------------------------------------------------------------------------

pub async fn get_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match resolve_workflow(&state, &id).await {
        Ok(wf) => (StatusCode::OK, Json(serde_json::to_value(wf).unwrap())).into_response(),
        Err((status, body)) => (status, body).into_response(),
    }
}

// ---------------------------------------------------------------------------
// GET /api/cost/workflows
// ---------------------------------------------------------------------------

pub async fn list_workflow_costs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CostWindowParams>,
) -> impl IntoResponse {
    let display_tz: Tz = match state.config.display_timezone.parse() {
        Ok(tz) => tz,
        Err(_) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!(
                    "Invalid display_timezone in daemon config: {}",
                    state.config.display_timezone
                ),
            )
            .into_response();
        }
    };

    let (since, until) = match resolve_window(&params, display_tz) {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    let workflows = match state.workflow_store.list_workflows().await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("Failed to list workflows for cost: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            )
            .into_response();
        }
    };

    let mut entries = Vec::with_capacity(workflows.len());
    let mut system_30d_total = 0.0f64;
    let mut system_30d_runs = 0u64;
    let mut system_year_total = 0.0f64;
    let mut system_year_runs = 0u64;
    let mut system_30d_input_tokens = 0u64;
    let mut system_30d_output_tokens = 0u64;
    let mut system_year_input_tokens = 0u64;
    let mut system_year_output_tokens = 0u64;

    for wf in &workflows {
        let mut cost_summary = match state.cost_cache.get(wf.id).await {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(workflow_id = %wf.id, "cost_cache.get failed: {}", e);
                continue;
            }
        };
        match state
            .cost_cache
            .get_daily_buckets_for(wf.id, since, until)
            .await
        {
            Ok(buckets) => cost_summary.daily_buckets = buckets,
            Err(e) => {
                tracing::warn!(workflow_id = %wf.id, "cost_cache.get_daily_buckets_for failed: {}", e);
            }
        }
        system_30d_total += cost_summary.last_30_days_total_usd;
        system_30d_runs += cost_summary.last_30_days_runs;
        system_year_total += cost_summary.last_year_total_usd;
        system_year_runs += cost_summary.last_year_runs;
        system_30d_input_tokens += cost_summary.last_30_days_input_tokens;
        system_30d_output_tokens += cost_summary.last_30_days_output_tokens;
        system_year_input_tokens += cost_summary.last_year_input_tokens;
        system_year_output_tokens += cost_summary.last_year_output_tokens;
        entries.push(WorkflowCostEntry {
            workflow_id: wf.id,
            workflow_name: wf.name.clone(),
            cost_summary,
        });
    }

    let system_buckets = match state
        .cost_cache
        .get_system_daily_buckets(since, until)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("cost_cache.get_system_daily_buckets failed: {}", e);
            vec![]
        }
    };

    let system_cost_summary = CostSummary {
        last_30_days_total_usd: system_30d_total,
        last_30_days_runs: system_30d_runs,
        last_year_total_usd: system_year_total,
        last_year_runs: system_year_runs,
        computed_at: Utc::now(),
        daily_buckets: system_buckets,
        last_30_days_input_tokens: system_30d_input_tokens,
        last_30_days_output_tokens: system_30d_output_tokens,
        last_year_input_tokens: system_year_input_tokens,
        last_year_output_tokens: system_year_output_tokens,
        last_30_days_avg_cost_per_run_usd: crate::models::workflow::avg_cost_per_run(
            system_30d_total,
            system_30d_runs,
        ),
        last_year_avg_cost_per_run_usd: crate::models::workflow::avg_cost_per_run(
            system_year_total,
            system_year_runs,
        ),
    };

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(CostWorkflowsListResponse {
                workflows: entries,
                system_cost_summary,
            })
            .unwrap(),
        ),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /api/cost/workflows/{id}
// ---------------------------------------------------------------------------

pub async fn get_workflow_costs(
    State(state): State<Arc<AppState>>,
    Path(workflow_id): Path<Uuid>,
    Query(params): Query<CostWindowParams>,
) -> impl IntoResponse {
    let display_tz: Tz = match state.config.display_timezone.parse() {
        Ok(tz) => tz,
        Err(_) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!(
                    "Invalid display_timezone in daemon config: {}",
                    state.config.display_timezone
                ),
            )
            .into_response();
        }
    };

    let (since, until) = match resolve_window(&params, display_tz) {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    let workflow = match state.workflow_store.get_workflow(workflow_id).await {
        Ok(Some(wf)) => wf,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("Workflow with id '{}' not found", workflow_id),
            )
            .into_response();
        }
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            )
            .into_response();
        }
    };

    let mut cost_summary = match state.cost_cache.get(workflow_id).await {
        Ok(s) => s,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &e.to_string(),
            )
            .into_response();
        }
    };

    match state
        .cost_cache
        .get_daily_buckets_for(workflow_id, since, until)
        .await
    {
        Ok(buckets) => cost_summary.daily_buckets = buckets,
        Err(e) => {
            tracing::warn!(workflow_id = %workflow_id, "cost_cache.get_daily_buckets_for failed: {}", e);
        }
    }

    (
        StatusCode::OK,
        Json(
            serde_json::to_value(CostWorkflowResponse {
                workflow_id,
                workflow_name: workflow.name,
                cost_summary,
            })
            .unwrap(),
        ),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// PATCH /api/workflows/{id}
// ---------------------------------------------------------------------------

pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(raw): Json<serde_json::Value>,
) -> impl IntoResponse {
    // Reject payloads that reference the removed command_template field.
    if let Some(steps) = raw.get("steps") {
        if steps_contain_command_template(steps) {
            return error_response(
                StatusCode::BAD_REQUEST,
                COMMAND_TEMPLATE_REMOVED_ERROR,
                COMMAND_TEMPLATE_REMOVED_MESSAGE,
            )
            .into_response();
        }
    }

    let body: WorkflowUpdate = match serde_json::from_value(raw) {
        Ok(b) => b,
        Err(e) => {
            return error_response(
                StatusCode::UNPROCESSABLE_ENTITY,
                "deserialization_error",
                &e.to_string(),
            )
            .into_response();
        }
    };

    let wf = match resolve_workflow(&state, &id).await {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    // Capture which fields the caller is updating (presence-based, no values).
    let mut changed: Vec<&'static str> = Vec::new();
    if body.name.is_some() {
        changed.push("name");
    }
    if body.schedule.is_some() {
        changed.push("schedule");
    }
    if body.timezone.is_some() {
        changed.push("timezone");
    }
    if body.schedule_mode.is_some() {
        changed.push("schedule_mode");
    }
    if body.enabled.is_some() {
        changed.push("enabled");
    }
    if body.steps.is_some() {
        changed.push("steps");
    }
    if body.default_input.is_some() {
        changed.push("default_input");
    }
    if body.working_dir.is_some() {
        changed.push("working_dir");
    }
    if body.env_vars.is_some() {
        changed.push("env_vars");
    }
    if body.allow_concurrent.is_some() {
        changed.push("allow_concurrent");
    }
    if body.on_failure.is_some() {
        changed.push("on_failure");
    }

    match state.workflow_store.update_workflow(wf.id, body).await {
        Ok(updated) => {
            // Broadcast WorkflowChanged{Updated}
            let _ = state
                .workflow_event_tx
                .send(WorkflowEvent::WorkflowChanged {
                    workflow_id: updated.id,
                    version: updated.version,
                    change_kind: WorkflowChangeKind::Updated,
                });
            tracing::info!(
                workflow_id = %updated.id,
                name = %updated.name,
                fields = ?changed,
                "workflow updated"
            );
            (
                StatusCode::OK,
                Json(serde_json::to_value(&updated).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            let (status, body) = map_store_error(e);
            (status, body).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/workflows/{id}
// ---------------------------------------------------------------------------

pub async fn delete_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let wf = match resolve_workflow(&state, &id).await {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    let workflow_id = wf.id;
    let version = wf.version;
    let workflow_name = wf.name.clone();

    match state.workflow_store.delete_workflow(workflow_id).await {
        Ok(()) => {
            // Evict cache entry for the deleted workflow.
            state.cost_cache.forget(workflow_id).await;
            // Broadcast WorkflowChanged{Deleted}
            let _ = state
                .workflow_event_tx
                .send(WorkflowEvent::WorkflowChanged {
                    workflow_id,
                    version,
                    change_kind: WorkflowChangeKind::Deleted,
                });
            tracing::info!(workflow_id = %workflow_id, name = %workflow_name, "workflow deleted");
            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            let (status, body) = map_store_error(e);
            (status, body).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/workflows/{id}/trigger
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TriggerResponse {
    run_id: Uuid,
    workflow_id: Uuid,
    workflow_version: u32,
    run_url: String,
}

/// 409 response body returned when a workflow has `allow_concurrent: false`
/// and a run is already active.
#[derive(Debug, Serialize)]
struct ConcurrentRunResponse {
    error: &'static str,
    message: &'static str,
    active_run_id: Uuid,
}

pub async fn trigger_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(params): Json<TriggerParams>,
) -> impl IntoResponse {
    let workflow = match resolve_workflow(&state, &id).await {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    // If allow_concurrent is false and a run is already active for this
    // workflow, reject with 409 Conflict instead of starting a new run.
    if !workflow.allow_concurrent {
        match state.workflow_run_store.list_runs(workflow.id, 0, 0).await {
            Ok(runs) => {
                if let Some(active) = runs.iter().find(|r| r.status == RunStatus::Running) {
                    return (
                        StatusCode::CONFLICT,
                        Json(ConcurrentRunResponse {
                            error: "concurrent_run_active",
                            message:
                                "Workflow already has a running run; concurrent runs are disabled.",
                            active_run_id: active.run_id,
                        }),
                    )
                        .into_response();
                }
            }
            Err(e) => {
                tracing::error!(
                    workflow_id = %workflow.id,
                    "Failed to list runs for allow_concurrent check: {}",
                    e
                );
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    &format!("Failed to check active runs: {}", e),
                )
                .into_response();
            }
        }
    }

    let run_id = Uuid::now_v7();
    let workflow_id = workflow.id;
    let workflow_version = workflow.version;
    let workflow_name = workflow.name.clone();

    // Resolve the data_dir for the log file.
    let data_dir = if let Some(ref dir) = state.config.data_dir {
        std::path::PathBuf::from(dir)
    } else {
        crate::daemon::resolve_data_dir(None)
    };

    let log_dir = data_dir.join("logs").join(workflow_id.to_string());
    let log_path = log_dir.join(format!("{}.log", run_id));

    // Persist a Running WorkflowRun to the store immediately so it's readable
    // before the background task completes.
    let initial_run = WorkflowRun {
        run_id,
        workflow_id,
        workflow_version,
        workflow_snapshot: workflow.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: RunStatus::Running,
        trigger_input: if params.input.is_null() {
            None
        } else {
            Some(params.input.clone())
        },
        steps: vec![],
        total_cost_usd: None,
        total_duration_ms: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
    };

    if let Err(e) = state.workflow_run_store.create_run(initial_run).await {
        tracing::error!("Failed to persist initial run {}: {}", run_id, e);
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to create run record: {}", e),
        )
        .into_response();
    }

    let event_tx = state.workflow_event_tx.clone();
    let run_store = Arc::clone(&state.workflow_run_store);
    let workflow_store = Arc::clone(&state.workflow_store);
    let kill_signals = Arc::clone(&state.kill_signals);

    // Spawn the workflow run in the background.
    tokio::spawn(async move {
        // Ensure log directory exists.
        if let Err(e) = tokio::fs::create_dir_all(&log_dir).await {
            tracing::error!(
                "Failed to create log dir for workflow run {}: {}",
                run_id,
                e
            );
            return;
        }

        // Create file log sink, wrapped in an event-emitting sink so that
        // stdout/stderr chunks are broadcast to SSE subscribers in real time.
        let log_sink = match FileLogSink::create(log_path).await {
            Ok(sink) => {
                let emitting = EventEmittingLogSink::new(
                    Arc::new(sink) as Arc<dyn crate::workflow::LogSink>,
                    event_tx.clone(),
                    run_id,
                    workflow_id,
                );
                Arc::new(emitting) as Arc<dyn crate::workflow::LogSink>
            }
            Err(e) => {
                tracing::error!("Failed to create log sink for run {}: {}", run_id, e);
                return;
            }
        };

        let final_run = crate::workflow::run_workflow(
            &workflow,
            run_id,
            params,
            log_sink,
            Some(event_tx),
            Some(kill_signals),
        )
        .await;

        // Persist the final run state to the store and stamp the parent
        // workflow with this run's terminal status so list_workflows /
        // get_workflow surface it in last_run_*.
        crate::workflow::finalize_run(&run_store, &workflow_store, &final_run).await;
    });

    tracing::info!(
        workflow_id = %workflow_id,
        name = %workflow_name,
        run_id = %run_id,
        "workflow triggered"
    );

    (
        StatusCode::ACCEPTED,
        Json(TriggerResponse {
            run_url: format!("/api/runs/{}", run_id),
            run_id,
            workflow_id,
            workflow_version,
        }),
    )
        .into_response()
}

// ---------------------------------------------------------------------------
// GET /api/runs/{run_id}
// ---------------------------------------------------------------------------

pub async fn get_workflow_run(
    State(state): State<Arc<AppState>>,
    Path(run_id_str): Path<String>,
) -> impl IntoResponse {
    let run_id = match Uuid::parse_str(&run_id_str) {
        Ok(id) => id,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("Invalid run_id '{}'", run_id_str),
            )
            .into_response();
        }
    };

    match state.workflow_run_store.get_run(run_id).await {
        Ok(Some(run)) => (StatusCode::OK, Json(serde_json::to_value(run).unwrap())).into_response(),
        Ok(None) => error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("Run '{}' not found", run_id),
        )
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to get run {}: {}", run_id, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to fetch run: {}", e),
            )
            .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/runs/{run_id}/kill
// ---------------------------------------------------------------------------
//
// Terminates a running workflow run:
//   1. Looks up the kill_signals registry and sends `true` so that the
//      running step's select! loop terminates the process tree immediately.
//   2. Updates the persisted run record to status=Killed so that polling
//      callers see the right state right away.
//
// Race note: if the run finishes between step 1 and step 2, the executor
// will have already written the final status (Completed/Failed).  The
// handler's update_run call here would then overwrite that with Killed.
// This is an acceptable race — the kill was requested while the run was
// believed to be running and arrived marginally late.  The executor also
// removes the registry entry before writing its final status, so step 1
// would have been a no-op in that scenario.

#[derive(Serialize)]
struct KillResponse {
    message: &'static str,
}

pub async fn kill_workflow_run(
    State(state): State<Arc<AppState>>,
    Path(run_id_str): Path<String>,
) -> impl IntoResponse {
    let run_id = match Uuid::parse_str(&run_id_str) {
        Ok(id) => id,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("Invalid run_id '{}'", run_id_str),
            )
            .into_response();
        }
    };

    // 1. Check that the run exists (and return 404 early if not).
    match state.workflow_run_store.get_run(run_id).await {
        Ok(Some(mut run)) => {
            // 2. Send kill signal to the executor (best-effort — the entry may
            //    be absent if the run already finished between the get_run call
            //    and here).
            if let Some(kill_tx) = state.kill_signals.read().await.get(&run_id) {
                let _ = kill_tx.send(true);
                tracing::info!("Kill signal sent to executor for run {}", run_id);
            } else {
                tracing::info!(
                    "Kill signal requested for run {} but no active executor entry \
                     (run may have already finished)",
                    run_id
                );
            }

            // 3. Update the persisted record if it is still Running, so that
            //    pollers see Killed immediately without waiting for the executor
            //    to write its final status.
            //
            //    The parent workflow's `last_run_*` fields are intentionally
            //    NOT updated here. The executor's finalization path is the
            //    single writer for the workflow row — it runs a moment later
            //    and reflects the actually-final run status (which may be
            //    Killed, Completed, or Failed depending on the kill-vs-exit
            //    race). Letting one writer own the field eliminates the
            //    possibility of `run.status` and `workflow.last_run_status`
            //    disagreeing.
            if run.status == RunStatus::Running {
                run.status = RunStatus::Killed;
                run.finished_at = Some(Utc::now());
                if let Err(e) = state.workflow_run_store.update_run(&run).await {
                    tracing::error!("Failed to persist kill status for run {}: {}", run_id, e);
                    return error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "internal_error",
                        &format!("Failed to update run status: {}", e),
                    )
                    .into_response();
                }
            }
            tracing::info!(run_id = %run_id, "workflow run killed");
            (
                StatusCode::ACCEPTED,
                Json(KillResponse {
                    message: "Kill signal sent",
                }),
            )
                .into_response()
        }
        Ok(None) => error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("Run '{}' not found", run_id),
        )
        .into_response(),
        Err(e) => {
            tracing::error!("Failed to get run {} for kill: {}", run_id, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to fetch run: {}", e),
            )
            .into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/runs/{run_id}/log
// ---------------------------------------------------------------------------
//
// Returns the on-disk run log (or a single step's slice of it) as text/plain.
// The file lives at `<data_dir>/logs/<workflow_id>/<run_id>.log` and contains
// the concatenated stdout/stderr from every step. Each persisted `StepRun`
// frames its bytes via `log_byte_offset_start` and `log_byte_offset_end` —
// when `step_index` is supplied the handler reads exactly that range.

#[derive(Deserialize, Default)]
pub struct RunLogQuery {
    /// If supplied, return only the byte range belonging to this step index.
    step_index: Option<usize>,
}

/// Resolve the configured data directory the same way `trigger_workflow` does.
fn resolve_data_dir_for(state: &AppState) -> std::path::PathBuf {
    if let Some(ref dir) = state.config.data_dir {
        std::path::PathBuf::from(dir)
    } else {
        crate::daemon::resolve_data_dir(None)
    }
}

pub async fn get_run_log(
    State(state): State<Arc<AppState>>,
    Path(run_id_str): Path<String>,
    Query(query): Query<RunLogQuery>,
) -> impl IntoResponse {
    let run_id = match Uuid::parse_str(&run_id_str) {
        Ok(id) => id,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("Invalid run_id '{}'", run_id_str),
            )
            .into_response();
        }
    };

    let run = match state.workflow_run_store.get_run(run_id).await {
        Ok(Some(r)) => r,
        Ok(None) => {
            return error_response(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("Run '{}' not found", run_id),
            )
            .into_response();
        }
        Err(e) => {
            tracing::error!("Failed to fetch run {} for log: {}", run_id, e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to fetch run: {}", e),
            )
            .into_response();
        }
    };

    let data_dir = resolve_data_dir_for(&state);
    let log_path = data_dir
        .join("logs")
        .join(run.workflow_id.to_string())
        .join(format!("{}.log", run_id));

    if !log_path.exists() {
        return error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("Log file for run '{}' is not on disk", run_id),
        )
        .into_response();
    }

    // Step-scoped slice → resolve bounds against the StepRun.
    if let Some(idx) = query.step_index {
        let step = match run.steps.iter().find(|s| s.step_index == idx) {
            Some(s) => s,
            None => {
                return error_response(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("step_index {} not found on run '{}'", idx, run_id),
                )
                .into_response();
            }
        };

        let bytes = match read_step_slice(
            &log_path,
            step.log_byte_offset_start,
            step.log_byte_offset_end,
        )
        .await
        {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                // Recorded `log_byte_offset_end` points past the end of the
                // log file on disk — most likely the log file was truncated,
                // rotated, or replaced after the run was persisted. Surface
                // this as 422 so the caller can distinguish it from a
                // generic IO error.
                return error_response(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "log_offset_out_of_range",
                    &format!("log file shorter than recorded offset for step {}", idx),
                )
                .into_response();
            }
            Err(e) => {
                tracing::error!(
                    "Failed to read step slice for run {} step {}: {}",
                    run_id,
                    idx,
                    e
                );
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    &format!("Failed to read step log: {}", e),
                )
                .into_response();
            }
        };
        return (
            StatusCode::OK,
            [(
                axum::http::header::CONTENT_TYPE,
                "text/plain; charset=utf-8",
            )],
            bytes,
        )
            .into_response();
    }

    // Whole-file path — no step_index supplied. Stream the file to avoid
    // buffering multi-MB logs into memory.
    match tokio::fs::File::open(&log_path).await {
        Ok(file) => {
            let stream = tokio_util::io::ReaderStream::new(file);
            let body = axum::body::Body::from_stream(stream);
            (
                StatusCode::OK,
                [(
                    axum::http::header::CONTENT_TYPE,
                    "text/plain; charset=utf-8",
                )],
                body,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to open log file {:?}: {}", log_path, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to read log file: {}", e),
            )
            .into_response()
        }
    }
}

/// Read `[start, end)` from `path`. When `end` is `None` the step is still
/// running and we tail to end-of-file.
///
/// Returns `Err(UnexpectedEof)` when `start` exceeds the file length so the
/// handler can map it to a `422 log_offset_out_of_range` instead of returning
/// an empty body. Without this check, `seek` past EOF followed by `read_to_end`
/// silently produces `Ok(empty)`, which is indistinguishable from "step
/// produced no output".
async fn read_step_slice(
    path: &std::path::Path,
    start: u64,
    end: Option<u64>,
) -> std::io::Result<Vec<u8>> {
    use tokio::io::{AsyncReadExt, AsyncSeekExt};
    let mut file = tokio::fs::File::open(path).await?;
    let file_len = file.metadata().await?.len();
    if start > file_len {
        return Err(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            format!(
                "step start offset {} exceeds log file length {}",
                start, file_len
            ),
        ));
    }
    file.seek(std::io::SeekFrom::Start(start)).await?;

    let mut buf = Vec::new();
    match end {
        Some(end_offset) if end_offset > start => {
            let len = (end_offset - start) as usize;
            buf.resize(len, 0);
            file.read_exact(&mut buf).await?;
        }
        Some(_) => {
            // end <= start → empty slice.
        }
        None => {
            file.read_to_end(&mut buf).await?;
        }
    }
    Ok(buf)
}

// ---------------------------------------------------------------------------
// GET /api/workflows/{id}/runs
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListRunsQuery {
    limit: Option<usize>,
    offset: Option<usize>,
}

#[derive(Serialize)]
struct ListRunsResponse {
    runs: Vec<WorkflowRun>,
    total: usize,
}

pub async fn list_workflow_runs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<ListRunsQuery>,
) -> impl IntoResponse {
    let workflow = match resolve_workflow(&state, &id).await {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    let runs = match state
        .workflow_run_store
        .list_runs(workflow.id, limit, offset)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to list runs for workflow {}: {}", workflow.id, e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to list runs: {}", e),
            )
            .into_response();
        }
    };

    let total = match state.workflow_run_store.count_runs(workflow.id).await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to count runs for workflow {}: {}", workflow.id, e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to count runs: {}", e),
            )
            .into_response();
        }
    };

    (StatusCode::OK, Json(ListRunsResponse { runs, total })).into_response()
}

// ---------------------------------------------------------------------------
// GET /api/runs/recent
// ---------------------------------------------------------------------------

pub async fn list_recent_runs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListRunsQuery>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(20).min(100);
    let offset = params.offset.unwrap_or(0);

    let runs = match state
        .workflow_run_store
        .list_recent_runs(limit, offset)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to list recent runs: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to list recent runs: {}", e),
            )
            .into_response();
        }
    };

    let total = match state.workflow_run_store.count_all_runs().await {
        Ok(n) => n,
        Err(e) => {
            tracing::error!("Failed to count recent runs: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to count recent runs: {}", e),
            )
            .into_response();
        }
    };

    (StatusCode::OK, Json(ListRunsResponse { runs, total })).into_response()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::daemon::events::WorkflowEvent;
    use crate::errors::AcsError;
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, NewWorkflow, RunStatus, ShellStep, StepDef, StepDefCommon,
        Workflow, WorkflowRun, WorkflowUpdate,
    };
    use crate::models::DaemonConfig;
    use crate::storage::workflow_runs::WorkflowRunStore;
    use crate::storage::workflows::WorkflowStore;
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::{DateTime, Utc};
    use http_body_util::BodyExt;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::{broadcast, Mutex, Notify};
    use tower::ServiceExt;
    use uuid::Uuid;

    // ── In-memory WorkflowStore ───────────────────────────────────────────────

    struct InMemoryWorkflowStore {
        workflows: tokio::sync::RwLock<Vec<crate::models::workflow::Workflow>>,
    }

    impl InMemoryWorkflowStore {
        fn new() -> Self {
            Self {
                workflows: tokio::sync::RwLock::new(vec![]),
            }
        }
    }

    #[async_trait]
    impl WorkflowStore for InMemoryWorkflowStore {
        async fn list_workflows(&self) -> anyhow::Result<Vec<Workflow>> {
            Ok(self.workflows.read().await.clone())
        }
        async fn get_workflow(&self, id: Uuid) -> anyhow::Result<Option<Workflow>> {
            Ok(self
                .workflows
                .read()
                .await
                .iter()
                .find(|w| w.id == id)
                .cloned())
        }
        async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Workflow>> {
            Ok(self
                .workflows
                .read()
                .await
                .iter()
                .find(|w| w.name == name)
                .cloned())
        }
        async fn create_workflow(&self, new: NewWorkflow) -> anyhow::Result<Workflow> {
            // Mirror the real SqliteWorkflowStore: validate before persisting.
            crate::models::workflow::validate_new_workflow(&new)?;
            let mut wfs = self.workflows.write().await;
            if wfs.iter().any(|w| w.name == new.name) {
                return Err(crate::errors::AcsError::Conflict(format!(
                    "A workflow with name '{}' already exists",
                    new.name
                ))
                .into());
            }
            let now = Utc::now();
            let wf = Workflow {
                id: Uuid::now_v7(),
                name: new.name,
                version: 1,
                schedule: new.schedule,
                timezone: new.timezone,
                schedule_mode: new.schedule_mode,
                enabled: new.enabled,
                steps: new.steps,
                default_input: new.default_input,
                working_dir: new.working_dir,
                env_vars: new.env_vars,
                allow_concurrent: new.allow_concurrent.unwrap_or(true),
                on_failure: new.on_failure,
                last_run_at: None,
                last_run_status: None,
                last_run_id: None,
                next_run_at: None,
                created_at: now,
                updated_at: now,
            };
            wfs.push(wf.clone());
            Ok(wf)
        }
        async fn update_workflow(
            &self,
            id: Uuid,
            update: WorkflowUpdate,
        ) -> anyhow::Result<Workflow> {
            let mut wfs = self.workflows.write().await;
            let idx = wfs
                .iter()
                .position(|w| w.id == id)
                .ok_or_else(|| anyhow::anyhow!("not found"))?;
            if let Some(ref new_name) = update.name {
                if wfs.iter().any(|w| w.name == *new_name && w.id != id) {
                    return Err(crate::errors::AcsError::Conflict(format!(
                        "A workflow with name '{}' already exists",
                        new_name
                    ))
                    .into());
                }
            }
            let wf = &mut wfs[idx];
            let mut bumped = false;
            if let Some(n) = update.name {
                if wf.name != n {
                    bumped = true;
                }
                wf.name = n;
            }
            if let Some(s) = update.schedule {
                if wf.schedule != s {
                    bumped = true;
                }
                wf.schedule = s;
            }
            if let Some(e) = update.enabled {
                wf.enabled = e;
            }
            if let Some(steps) = update.steps {
                if wf.steps != steps {
                    bumped = true;
                }
                wf.steps = steps;
            }
            if bumped {
                wf.version += 1;
            }
            wf.updated_at = Utc::now();
            Ok(wf.clone())
        }
        async fn delete_workflow(&self, id: Uuid) -> anyhow::Result<()> {
            let mut wfs = self.workflows.write().await;
            let len_before = wfs.len();
            wfs.retain(|w| w.id != id);
            if wfs.len() == len_before {
                return Err(anyhow::anyhow!("not found"));
            }
            Ok(())
        }
        async fn record_run_outcome(
            &self,
            workflow_id: Uuid,
            run_id: Uuid,
            status: RunStatus,
            finished_at: DateTime<Utc>,
        ) -> anyhow::Result<()> {
            let mut wfs = self.workflows.write().await;
            let wf = wfs
                .iter_mut()
                .find(|w| w.id == workflow_id)
                .ok_or_else(|| {
                    crate::errors::AcsError::NotFound(format!(
                        "Workflow with id '{}' not found",
                        workflow_id
                    ))
                })?;
            wf.last_run_id = Some(run_id);
            wf.last_run_status = Some(status);
            wf.last_run_at = Some(finished_at);
            wf.updated_at = Utc::now();
            Ok(())
        }
    }

    // ── In-memory WorkflowRunStore ────────────────────────────────────────────

    struct InMemoryRunStore {
        runs: Mutex<HashMap<Uuid, WorkflowRun>>,
    }

    impl InMemoryRunStore {
        fn new() -> Self {
            Self {
                runs: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl WorkflowRunStore for InMemoryRunStore {
        async fn create_run(&self, run: WorkflowRun) -> Result<(), AcsError> {
            self.runs.lock().await.insert(run.run_id, run);
            Ok(())
        }
        async fn update_run(&self, run: &WorkflowRun) -> Result<(), AcsError> {
            let mut map = self.runs.lock().await;
            if !map.contains_key(&run.run_id) {
                return Err(AcsError::NotFound(format!(
                    "Run '{}' not found",
                    run.run_id
                )));
            }
            map.insert(run.run_id, run.clone());
            Ok(())
        }
        async fn get_run(&self, run_id: Uuid) -> Result<Option<WorkflowRun>, AcsError> {
            Ok(self.runs.lock().await.get(&run_id).cloned())
        }
        async fn list_runs(
            &self,
            workflow_id: Uuid,
            limit: usize,
            offset: usize,
        ) -> Result<Vec<WorkflowRun>, AcsError> {
            let map = self.runs.lock().await;
            let mut runs: Vec<WorkflowRun> = map
                .values()
                .filter(|r| r.workflow_id == workflow_id)
                .cloned()
                .collect();
            runs.sort_by(|a, b| b.run_id.cmp(&a.run_id));
            let result = if limit == 0 {
                runs.into_iter().skip(offset).collect()
            } else {
                runs.into_iter().skip(offset).take(limit).collect()
            };
            Ok(result)
        }
        async fn count_runs(&self, workflow_id: Uuid) -> Result<usize, AcsError> {
            Ok(self
                .runs
                .lock()
                .await
                .values()
                .filter(|r| r.workflow_id == workflow_id)
                .count())
        }
        async fn list_recent_runs(
            &self,
            limit: usize,
            offset: usize,
        ) -> Result<Vec<WorkflowRun>, AcsError> {
            let map = self.runs.lock().await;
            let mut runs: Vec<WorkflowRun> = map.values().cloned().collect();
            runs.sort_by(|a, b| b.run_id.cmp(&a.run_id));
            let result = if limit == 0 {
                runs.into_iter().skip(offset).collect()
            } else {
                runs.into_iter().skip(offset).take(limit).collect()
            };
            Ok(result)
        }
        async fn count_all_runs(&self) -> Result<usize, AcsError> {
            Ok(self.runs.lock().await.len())
        }
        async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError> {
            self.runs.lock().await.remove(&run_id);
            Ok(())
        }
        async fn cost_summary_for(
            &self,
            _workflow_id: Uuid,
            _display_tz: &chrono_tz::Tz,
        ) -> Result<crate::models::workflow::CostSummary, AcsError> {
            Ok(crate::models::workflow::CostSummary {
                last_30_days_total_usd: 0.0,
                last_30_days_runs: 0,
                last_year_total_usd: 0.0,
                last_year_runs: 0,
                computed_at: chrono::Utc::now(),
                daily_buckets: Vec::new(),
                last_30_days_input_tokens: 0,
                last_30_days_output_tokens: 0,
                last_year_input_tokens: 0,
                last_year_output_tokens: 0,
                last_30_days_avg_cost_per_run_usd: 0.0,
                last_year_avg_cost_per_run_usd: 0.0,
            })
        }
        async fn daily_buckets_for(
            &self,
            _workflow_id: Option<Uuid>,
            _since: chrono::DateTime<chrono::Utc>,
            _until: chrono::DateTime<chrono::Utc>,
            _display_tz: &chrono_tz::Tz,
        ) -> Result<Vec<crate::models::workflow::DailyBucket>, AcsError> {
            Ok(Vec::new())
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn make_shell_step(id: &str) -> StepDef {
        StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(30),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: "echo hello".to_string(),
            pass_stdin: false,
        })
    }

    fn make_new_workflow(name: &str) -> NewWorkflow {
        NewWorkflow {
            name: name.to_string(),
            schedule: "*/5 * * * *".to_string(),
            timezone: None,
            schedule_mode: Default::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: None,
            on_failure: FailurePolicy::default(),
        }
    }

    fn make_state(
        wf_store: Arc<dyn WorkflowStore>,
        run_store: Arc<dyn WorkflowRunStore>,
    ) -> Arc<crate::server::AppState> {
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(256);
        // Point the default-working-dir root at a per-process tempdir under
        // std::env::temp_dir() so unit tests don't leak directories into the
        // user's real Documents folder. The tempdir is unique per process so
        // parallel test workers don't collide.
        let mut config = DaemonConfig::default();
        let test_root = std::env::temp_dir()
            .join("acs-unit-test-workflow-dirs")
            .join(format!("p{}", std::process::id()));
        // Best-effort create — failures are non-fatal since
        // `apply_new_workflow_defaults` will call `create_dir_all` again on
        // each sub-path it needs.
        let _ = std::fs::create_dir_all(&test_root);
        config.display_workflow_dir_root = Some(test_root);
        let display_tz: chrono_tz::Tz = "America/Los_Angeles".parse().unwrap();
        let cost_cache = Arc::new(crate::daemon::cost_cache::CostCache::new(
            Arc::clone(&run_store),
            display_tz,
        ));
        Arc::new(crate::server::AppState {
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(config),
            start_time: Instant::now(),
            shutdown_tx: None,
            workflow_event_tx,
            workflow_store: wf_store,
            workflow_run_store: run_store,
            kill_signals: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            cost_cache,
        })
    }

    fn make_state_default(wf_store: Arc<dyn WorkflowStore>) -> Arc<crate::server::AppState> {
        make_state(
            wf_store,
            Arc::new(InMemoryRunStore::new()) as Arc<dyn WorkflowRunStore>,
        )
    }

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_workflows_empty() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/workflows")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        // v4.2.10: response reverted to plain array
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_create_workflow_returns_201() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        let body = serde_json::to_string(&make_new_workflow("test-wf")).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workflows")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["name"], "test-wf");
        assert_eq!(json["version"], 1);
    }

    #[tokio::test]
    async fn test_create_workflow_fills_default_timezone_from_config() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        // make_new_workflow leaves timezone = None.
        let body = serde_json::to_string(&make_new_workflow("tz-default")).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workflows")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp.into_body()).await;
        assert_eq!(
            json["timezone"], "America/Los_Angeles",
            "missing timezone must default to the daemon-config display_timezone"
        );
    }

    #[tokio::test]
    async fn test_create_workflow_fills_default_working_dir_and_creates_it() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let configured_root = state
            .config
            .display_workflow_dir_root
            .clone()
            .expect("test state sets display_workflow_dir_root");
        let app = crate::server::create_router(state);
        let unique_name = format!("wd-default-{}", Uuid::now_v7());
        let body = serde_json::to_string(&make_new_workflow(&unique_name)).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workflows")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp.into_body()).await;
        let wd = json["working_dir"]
            .as_str()
            .expect("working_dir must be auto-filled when config root is set");
        let wd_path = std::path::Path::new(wd);
        assert!(
            wd_path.starts_with(&configured_root),
            "default working_dir {:?} must live under the configured root {:?}",
            wd_path,
            configured_root
        );
        assert!(
            wd_path.exists(),
            "default working_dir should be created on disk: {}",
            wd
        );
        // Best-effort cleanup so the temp area doesn't accumulate across runs.
        let _ = std::fs::remove_dir_all(wd);
    }

    #[tokio::test]
    async fn test_create_workflow_preserves_explicit_working_dir() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        let mut nw = make_new_workflow("wd-explicit");
        let explicit_path = "/some/specific/path";
        nw.working_dir = Some(explicit_path.to_string());
        let body = serde_json::to_string(&nw).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workflows")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        let json = body_json(resp.into_body()).await;
        assert_eq!(
            json["working_dir"], explicit_path,
            "explicit working_dir must round-trip verbatim"
        );
        // The default-fill path is not invoked when the caller supplies a
        // value, so this nonsensical path must NOT have been created on disk.
        assert!(
            !std::path::Path::new(explicit_path).exists(),
            "explicit working_dir must not be auto-created"
        );
    }

    #[tokio::test]
    async fn test_create_workflow_duplicate_name_returns_409() {
        let store = Arc::new(InMemoryWorkflowStore::new());
        let state = make_state_default(Arc::clone(&store) as Arc<dyn WorkflowStore>);
        // Create once
        store
            .create_workflow(make_new_workflow("dup-wf"))
            .await
            .unwrap();
        let app = crate::server::create_router(state);
        let body = serde_json::to_string(&make_new_workflow("dup-wf")).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workflows")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn test_get_workflow_by_id() {
        let store = Arc::new(InMemoryWorkflowStore::new());
        let wf = store
            .create_workflow(make_new_workflow("get-by-id"))
            .await
            .unwrap();
        let state = make_state_default(Arc::clone(&store) as Arc<dyn WorkflowStore>);
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(&format!("/api/workflows/{}", wf.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["id"], wf.id.to_string());
    }

    #[tokio::test]
    async fn test_get_workflow_by_name() {
        let store = Arc::new(InMemoryWorkflowStore::new());
        let _wf = store
            .create_workflow(make_new_workflow("by-name-wf"))
            .await
            .unwrap();
        let state = make_state_default(Arc::clone(&store) as Arc<dyn WorkflowStore>);
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/workflows/by-name-wf")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["name"], "by-name-wf");
    }

    #[tokio::test]
    async fn test_get_workflow_not_found() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(&format!("/api/workflows/{}", Uuid::now_v7()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_delete_workflow_returns_204() {
        let store = Arc::new(InMemoryWorkflowStore::new());
        let wf = store
            .create_workflow(make_new_workflow("to-delete"))
            .await
            .unwrap();
        let state = make_state_default(Arc::clone(&store) as Arc<dyn WorkflowStore>);
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&format!("/api/workflows/{}", wf.id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        // Verify it's gone
        assert!(store.get_workflow(wf.id).await.unwrap().is_none());
    }

    // ── get_workflow_run via store ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_run_not_found_returns_404() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(&format!("/api/runs/{}", Uuid::now_v7()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_run_returns_stored_run() {
        let run_store = Arc::new(InMemoryRunStore::new());
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("run-test"))
            .await
            .unwrap();

        // Pre-insert a run.
        let run = WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: wf.id,
            workflow_version: 1,
            workflow_snapshot: wf.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        let run_id = run.run_id;
        run_store.create_run(run).await.unwrap();

        let state = make_state(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
        );
        let app = crate::server::create_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri(&format!("/api/runs/{}", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["run_id"], run_id.to_string());
        assert_eq!(json["status"], "Running");
    }

    #[tokio::test]
    async fn test_create_workflow_invalid_cron_returns_422() {
        let state = make_state_default(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        // Use an invalid cron expression to trigger AcsError::Cron.
        let mut bad_wf = make_new_workflow("bad-cron-wf");
        bad_wf.schedule = "not a cron".to_string();
        let body = serde_json::to_string(&bad_wf).unwrap();
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/workflows")
                    .header("Content-Type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["error"], "validation_error");
        assert!(
            json["message"].as_str().unwrap().contains("cron"),
            "expected cron error in message, got: {}",
            json["message"]
        );
    }

    // ── Trigger / allow_concurrent reject semantics ───────────────────────────

    /// Build a NewWorkflow with `allow_concurrent: false` so the trigger
    /// endpoint enforces the concurrency guard.
    fn make_new_workflow_no_concurrent(name: &str) -> NewWorkflow {
        NewWorkflow {
            name: name.to_string(),
            schedule: "*/5 * * * *".to_string(),
            timezone: None,
            schedule_mode: Default::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: Some(false),
            on_failure: FailurePolicy::default(),
        }
    }

    /// Insert a `Running` run for the given workflow into the store. Used to
    /// simulate an in-flight run when testing the trigger endpoint's
    /// concurrency guard.
    async fn insert_running_run(run_store: &InMemoryRunStore, wf: &Workflow) -> Uuid {
        let run = WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: wf.id,
            workflow_version: wf.version,
            workflow_snapshot: wf.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        let id = run.run_id;
        run_store.create_run(run).await.unwrap();
        id
    }

    #[tokio::test]
    async fn test_trigger_returns_409_when_allow_concurrent_false_and_run_active() {
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow_no_concurrent("no-concurrent-wf"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());
        let active_run_id = insert_running_run(&run_store, &wf).await;

        let state = make_state(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
        );
        let app = crate::server::create_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/workflows/{}/trigger", wf.id))
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::CONFLICT);
        let json = body_json(resp.into_body()).await;
        assert_eq!(json["error"], "concurrent_run_active");
        assert_eq!(
            json["message"],
            "Workflow already has a running run; concurrent runs are disabled."
        );
        assert_eq!(json["active_run_id"], active_run_id.to_string());

        // No new run was created — the store should still contain only the
        // single active run we inserted.
        let total = run_store.count_runs(wf.id).await.unwrap();
        assert_eq!(total, 1, "no additional run should have been created");
    }

    #[tokio::test]
    async fn test_trigger_returns_202_when_allow_concurrent_true_and_run_active() {
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        // Default (allow_concurrent: None → true)
        let wf = wf_store
            .create_workflow(make_new_workflow("concurrent-ok-wf"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());
        let _active_run_id = insert_running_run(&run_store, &wf).await;

        let state = make_state(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
        );
        let app = crate::server::create_router(state);

        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/workflows/{}/trigger", wf.id))
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::ACCEPTED);
    }

    #[tokio::test]
    async fn test_trigger_succeeds_after_active_run_finishes() {
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow_no_concurrent("lock-release-wf"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        // Insert a Running run, then mark it Completed before triggering.
        let active_run_id = insert_running_run(&run_store, &wf).await;
        let mut active = run_store.get_run(active_run_id).await.unwrap().unwrap();
        active.status = RunStatus::Completed;
        active.finished_at = Some(Utc::now());
        run_store.update_run(&active).await.unwrap();

        let state = make_state(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
        );
        let app = crate::server::create_router(state);

        // Now the trigger should succeed because no run is Running anymore.
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&format!("/api/workflows/{}/trigger", wf.id))
                    .header("Content-Type", "application/json")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            resp.status(),
            StatusCode::ACCEPTED,
            "trigger should succeed after the active run finishes"
        );
    }

    // ── GET /api/runs/{run_id}/log ────────────────────────────────────────────

    /// Build an `AppState` whose `data_dir` points at the given directory so
    /// the log endpoint resolves files inside the test sandbox instead of the
    /// real platform data dir.
    fn make_state_with_data_dir(
        wf_store: Arc<dyn WorkflowStore>,
        run_store: Arc<dyn WorkflowRunStore>,
        data_dir: &std::path::Path,
    ) -> Arc<crate::server::AppState> {
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(256);
        let config = DaemonConfig {
            data_dir: Some(data_dir.to_path_buf()),
            ..DaemonConfig::default()
        };
        let display_tz: chrono_tz::Tz = "America/Los_Angeles".parse().unwrap();
        let cost_cache = Arc::new(crate::daemon::cost_cache::CostCache::new(
            Arc::clone(&run_store),
            display_tz,
        ));
        Arc::new(crate::server::AppState {
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(config),
            start_time: Instant::now(),
            shutdown_tx: None,
            workflow_event_tx,
            workflow_store: wf_store,
            workflow_run_store: run_store,
            kill_signals: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            cost_cache,
        })
    }

    /// Stage a run record + on-disk log file inside `data_dir`.
    async fn stage_run_with_log(
        run_store: &InMemoryRunStore,
        wf: &Workflow,
        steps: Vec<crate::models::workflow::StepRun>,
        log_bytes: &[u8],
        data_dir: &std::path::Path,
    ) -> Uuid {
        let run_id = Uuid::now_v7();
        let run = WorkflowRun {
            run_id,
            workflow_id: wf.id,
            workflow_version: wf.version,
            workflow_snapshot: wf.clone(),
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            status: RunStatus::Completed,
            trigger_input: None,
            steps,
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        run_store.create_run(run).await.unwrap();

        let log_dir = data_dir.join("logs").join(wf.id.to_string());
        tokio::fs::create_dir_all(&log_dir).await.unwrap();
        tokio::fs::write(log_dir.join(format!("{}.log", run_id)), log_bytes)
            .await
            .unwrap();
        run_id
    }

    fn step_record(
        index: usize,
        id: &str,
        start: u64,
        end: Option<u64>,
    ) -> crate::models::workflow::StepRun {
        crate::models::workflow::StepRun {
            step_index: index,
            step_id: id.to_string(),
            kind: "shell".to_string(),
            status: RunStatus::Completed,
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            exit_code: Some(0),
            log_byte_offset_start: start,
            log_byte_offset_end: end,
            cost_usd: None,
            error: None,
        }
    }

    #[tokio::test]
    async fn test_get_run_log_returns_full_file_when_no_step_index() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("log-full"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        let log_bytes = b"step0-bytes-step1-bytes";
        let steps = vec![
            step_record(0, "s0", 0, Some(11)),
            step_record(1, "s1", 11, Some(23)),
        ];
        let run_id = stage_run_with_log(&run_store, &wf, steps, log_bytes, tmp.path()).await;

        let state = make_state_with_data_dir(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], log_bytes);
    }

    #[tokio::test]
    async fn test_get_run_log_returns_step_slice() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("log-slice"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        let log_bytes = b"AAAAABBBBBCCCCC";
        let steps = vec![
            step_record(0, "s0", 0, Some(5)),
            step_record(1, "s1", 5, Some(10)),
            step_record(2, "s2", 10, Some(15)),
        ];
        let run_id = stage_run_with_log(&run_store, &wf, steps, log_bytes, tmp.path()).await;

        let state = make_state_with_data_dir(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log?step_index=1", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&bytes[..], b"BBBBB");
    }

    #[tokio::test]
    async fn test_get_run_log_step_index_in_progress_tails_to_eof() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("log-in-progress"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        // Partial log: a START marker and some output, but no END marker yet
        // (i.e. the step is still running or the daemon crashed mid-step).
        let log_bytes = b"===== START =====\n some content";
        let steps = vec![crate::models::workflow::StepRun {
            step_index: 0,
            step_id: "s0".to_string(),
            kind: "shell".to_string(),
            status: RunStatus::Running,
            started_at: Utc::now(),
            finished_at: None,
            exit_code: None,
            log_byte_offset_start: 0,
            log_byte_offset_end: None,
            cost_usd: None,
            error: None,
        }];
        // stage_run_with_log persists status=Completed; we override afterwards
        // by directly inserting a Running run.
        let run_id = Uuid::now_v7();
        let run = WorkflowRun {
            run_id,
            workflow_id: wf.id,
            workflow_version: wf.version,
            workflow_snapshot: wf.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps,
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        run_store.create_run(run).await.unwrap();
        let log_dir = tmp.path().join("logs").join(wf.id.to_string());
        tokio::fs::create_dir_all(&log_dir).await.unwrap();
        tokio::fs::write(log_dir.join(format!("{}.log", run_id)), log_bytes)
            .await
            .unwrap();

        let state = make_state_with_data_dir(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log?step_index=0", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            &bytes[..],
            log_bytes,
            "in-progress step (end=None) must tail the log to EOF"
        );
    }

    #[tokio::test]
    async fn test_get_run_log_offset_past_eof_returns_422() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("log-out-of-range"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        // Log file is 100 bytes, but the persisted StepRun's end offset is
        // 1_000_000 — wildly past EOF.
        let log_bytes = vec![b'X'; 100];
        let steps = vec![step_record(0, "s0", 0, Some(1_000_000))];
        let run_id = stage_run_with_log(&run_store, &wf, steps, &log_bytes, tmp.path()).await;

        let state = make_state_with_data_dir(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log?step_index=0", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body_bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["error"], "log_offset_out_of_range");
    }

    #[tokio::test]
    async fn test_get_run_log_step_index_not_found_returns_404() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("log-404-step"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        let steps = vec![step_record(0, "s0", 0, Some(3))];
        let run_id = stage_run_with_log(&run_store, &wf, steps, b"abc", tmp.path()).await;

        let state = make_state_with_data_dir(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log?step_index=99", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_run_log_unknown_run_returns_404() {
        let tmp = tempfile::TempDir::new().unwrap();
        let state = make_state_with_data_dir(
            Arc::new(InMemoryWorkflowStore::new()) as Arc<dyn WorkflowStore>,
            Arc::new(InMemoryRunStore::new()) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log", Uuid::now_v7()))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn test_get_run_log_missing_log_file_returns_404() {
        let tmp = tempfile::TempDir::new().unwrap();
        let wf_store = Arc::new(InMemoryWorkflowStore::new());
        let wf = wf_store
            .create_workflow(make_new_workflow("log-missing-file"))
            .await
            .unwrap();
        let run_store = Arc::new(InMemoryRunStore::new());

        // Insert a run record but never write the log file to disk.
        let run = WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: wf.id,
            workflow_version: wf.version,
            workflow_snapshot: wf.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        let run_id = run.run_id;
        run_store.create_run(run).await.unwrap();

        let state = make_state_with_data_dir(
            Arc::clone(&wf_store) as Arc<dyn WorkflowStore>,
            Arc::clone(&run_store) as Arc<dyn WorkflowRunStore>,
            tmp.path(),
        );
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(format!("/api/runs/{}/log", run_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
