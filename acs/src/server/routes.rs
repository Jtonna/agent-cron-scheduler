use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::daemon::events::{JobChangeKind, JobEvent};
use crate::models::job::{validate_job_update, validate_new_job};
use crate::models::{DispatchRequest, Job, JobUpdate, NewJob, TriggerParams};

// ---------------------------------------------------------------------------
// Error response
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: String,
    pub message: String,
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

// ---------------------------------------------------------------------------
// Job ID resolution: try UUID first, then name lookup
// ---------------------------------------------------------------------------

async fn resolve_job(
    state: &AppState,
    id_or_name: &str,
) -> Result<Job, (StatusCode, Json<ErrorResponse>)> {
    // Try UUID parse first
    if let Ok(uuid) = Uuid::parse_str(id_or_name) {
        match state.job_store.get_job(uuid).await {
            Ok(Some(job)) => return Ok(job),
            Ok(None) => {
                tracing::warn!("Job not found: '{}'", id_or_name);
                return Err((
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse {
                        error: "not_found".to_string(),
                        message: format!("Job with id '{}' not found", id_or_name),
                    }),
                ));
            }
            Err(e) => {
                tracing::warn!("Failed to fetch job '{}': {}", id_or_name, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse {
                        error: "internal_error".to_string(),
                        message: format!("Failed to fetch job: {}", e),
                    }),
                ));
            }
        }
    }

    // Not a valid UUID -- try name lookup
    match state.job_store.find_by_name(id_or_name).await {
        Ok(Some(job)) => Ok(job),
        Ok(None) => {
            tracing::warn!("Job not found: '{}'", id_or_name);
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse {
                    error: "not_found".to_string(),
                    message: format!("Job with name '{}' not found", id_or_name),
                }),
            ))
        }
        Err(e) => {
            tracing::warn!("Failed to fetch job '{}': {}", id_or_name, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse {
                    error: "internal_error".to_string(),
                    message: format!("Failed to fetch job: {}", e),
                }),
            ))
        }
    }
}

// ---------------------------------------------------------------------------
// Query params
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct ListJobsParams {
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ListRunsParams {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    pub status: Option<String>,
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Deserialize, Default)]
pub struct GetLogParams {
    pub tail: Option<usize>,
    pub format: Option<String>,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// GET /api/jobs
pub async fn list_jobs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ListJobsParams>,
) -> impl IntoResponse {
    match state.job_store.list_jobs().await {
        Ok(jobs) => {
            let mut filtered: Vec<Job> = match params.enabled {
                Some(enabled) => jobs.into_iter().filter(|j| j.enabled == enabled).collect(),
                None => jobs,
            };
            // Compute next_run_at for each job (it is #[serde(skip)] so not persisted)
            let now = Utc::now();
            for job in &mut filtered {
                if job.enabled {
                    if let Ok(next) = crate::daemon::scheduler::compute_next_run(
                        &job.schedule,
                        job.timezone.as_deref(),
                        now,
                    ) {
                        job.next_run_at = Some(next);
                    }
                }
            }
            (
                StatusCode::OK,
                Json(serde_json::to_value(&filtered).unwrap()),
            )
                .into_response()
        }
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to list jobs: {}", e),
        )
        .into_response(),
    }
}

/// POST /api/jobs
pub async fn create_job(
    State(state): State<Arc<AppState>>,
    Json(new_job): Json<NewJob>,
) -> impl IntoResponse {
    // Validate the new job
    if let Err(e) = validate_new_job(&new_job) {
        tracing::warn!("Job creation failed: {}", e);
        let (error_code, status) = ("validation_error", StatusCode::BAD_REQUEST);
        return error_response(status, error_code, &e.to_string()).into_response();
    }

    // Check duplicate name
    match state.job_store.find_by_name(&new_job.name).await {
        Ok(Some(_)) => {
            tracing::warn!(
                "Job creation failed: name '{}' already exists",
                new_job.name
            );
            return error_response(
                StatusCode::CONFLICT,
                "conflict",
                &format!("A job with name '{}' already exists", new_job.name),
            )
            .into_response();
        }
        Ok(None) => {} // good, no conflict
        Err(e) => {
            tracing::warn!("Job creation failed: {}", e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to check name uniqueness: {}", e),
            )
            .into_response();
        }
    }

    match state.job_store.create_job(new_job).await {
        Ok(job) => {
            tracing::info!("Job '{}' created (id: {})", job.name, job.id);

            // Broadcast JobChanged::Added
            let _ = state.event_tx.send(JobEvent::JobChanged {
                job_id: job.id,
                change: JobChangeKind::Added,
                timestamp: Utc::now(),
            });
            // Notify scheduler
            state.scheduler_notify.notify_one();

            (
                StatusCode::CREATED,
                Json(serde_json::to_value(&job).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            let err_str = e.to_string();
            tracing::warn!("Job creation failed: {}", err_str);
            if err_str.contains("already exists") || err_str.contains("Conflict") {
                error_response(StatusCode::CONFLICT, "conflict", &err_str).into_response()
            } else {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    &err_str,
                )
                .into_response()
            }
        }
    }
}

/// GET /api/jobs/{id}
pub async fn get_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match resolve_job(&state, &id).await {
        Ok(mut job) => {
            // Compute next_run_at (it is not persisted)
            if job.enabled {
                if let Ok(next) = crate::daemon::scheduler::compute_next_run(
                    &job.schedule,
                    job.timezone.as_deref(),
                    Utc::now(),
                ) {
                    job.next_run_at = Some(next);
                }
            }
            (StatusCode::OK, Json(serde_json::to_value(&job).unwrap())).into_response()
        }
        Err(resp) => resp.into_response(),
    }
}

/// PATCH /api/jobs/{id}
pub async fn update_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(update): Json<JobUpdate>,
) -> impl IntoResponse {
    // Resolve the job
    let job = match resolve_job(&state, &id).await {
        Ok(j) => j,
        Err(resp) => return resp.into_response(),
    };

    // Validate the update
    if let Err(e) = validate_job_update(&update) {
        tracing::warn!("Job update failed for '{}': {}", id, e);
        return error_response(StatusCode::BAD_REQUEST, "validation_error", &e.to_string())
            .into_response();
    }

    // Check name uniqueness (excluding self)
    if let Some(ref new_name) = update.name {
        match state.job_store.find_by_name(new_name).await {
            Ok(Some(existing)) if existing.id != job.id => {
                tracing::warn!("Job update failed: name '{}' already exists", new_name);
                return error_response(
                    StatusCode::CONFLICT,
                    "conflict",
                    &format!("A job with name '{}' already exists", new_name),
                )
                .into_response();
            }
            Err(e) => {
                tracing::warn!("Job update failed: {}", e);
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    &format!("Failed to check name uniqueness: {}", e),
                )
                .into_response();
            }
            _ => {}
        }
    }

    match state.job_store.update_job(job.id, update).await {
        Ok(updated) => {
            tracing::info!("Job '{}' updated (id: {})", updated.name, updated.id);

            // Broadcast JobChanged::Updated
            let _ = state.event_tx.send(JobEvent::JobChanged {
                job_id: updated.id,
                change: JobChangeKind::Updated,
                timestamp: Utc::now(),
            });
            state.scheduler_notify.notify_one();

            (
                StatusCode::OK,
                Json(serde_json::to_value(&updated).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            let err_str = e.to_string();
            tracing::warn!("Job update failed: {}", err_str);
            if err_str.contains("already exists") || err_str.contains("Conflict") {
                error_response(StatusCode::CONFLICT, "conflict", &err_str).into_response()
            } else {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    &err_str,
                )
                .into_response()
            }
        }
    }
}

/// DELETE /api/jobs/{id}
pub async fn delete_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let job = match resolve_job(&state, &id).await {
        Ok(j) => j,
        Err(resp) => return resp.into_response(),
    };

    // Kill active run if any
    {
        let mut runs = state.active_runs.write().await;
        if let Some(handle) = runs.remove(&job.id) {
            let _ = handle.kill_tx.send(());
        }
    }

    match state.job_store.delete_job(job.id).await {
        Ok(()) => {
            tracing::info!("Job '{}' deleted (id: {})", job.name, job.id);

            // Broadcast JobChanged::Removed
            let _ = state.event_tx.send(JobEvent::JobChanged {
                job_id: job.id,
                change: JobChangeKind::Removed,
                timestamp: Utc::now(),
            });
            state.scheduler_notify.notify_one();

            StatusCode::NO_CONTENT.into_response()
        }
        Err(e) => {
            tracing::warn!("Job deletion failed for '{}': {}", job.name, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to delete job: {}", e),
            )
            .into_response()
        }
    }
}

/// POST /api/jobs/{id}/enable
pub async fn enable_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let job = match resolve_job(&state, &id).await {
        Ok(j) => j,
        Err(resp) => return resp.into_response(),
    };

    let update = JobUpdate {
        enabled: Some(true),
        ..Default::default()
    };

    match state.job_store.update_job(job.id, update).await {
        Ok(updated) => {
            tracing::info!("Job '{}' enabled", updated.name);

            let _ = state.event_tx.send(JobEvent::JobChanged {
                job_id: updated.id,
                change: JobChangeKind::Enabled,
                timestamp: Utc::now(),
            });
            state.scheduler_notify.notify_one();

            (
                StatusCode::OK,
                Json(serde_json::to_value(&updated).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("Failed to enable job '{}': {}", job.name, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to enable job: {}", e),
            )
            .into_response()
        }
    }
}

/// POST /api/jobs/{id}/disable
pub async fn disable_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let job = match resolve_job(&state, &id).await {
        Ok(j) => j,
        Err(resp) => return resp.into_response(),
    };

    let update = JobUpdate {
        enabled: Some(false),
        ..Default::default()
    };

    match state.job_store.update_job(job.id, update).await {
        Ok(updated) => {
            tracing::info!("Job '{}' disabled", updated.name);

            let _ = state.event_tx.send(JobEvent::JobChanged {
                job_id: updated.id,
                change: JobChangeKind::Disabled,
                timestamp: Utc::now(),
            });
            state.scheduler_notify.notify_one();

            (
                StatusCode::OK,
                Json(serde_json::to_value(&updated).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            tracing::warn!("Failed to disable job '{}': {}", job.name, e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to disable job: {}", e),
            )
            .into_response()
        }
    }
}

/// POST /api/jobs/{id}/trigger
pub async fn trigger_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let job = match resolve_job(&state, &id).await {
        Ok(j) => j,
        Err(resp) => return resp.into_response(),
    };

    // Parse optional trigger params from body
    let trigger_params: Option<TriggerParams> = if body.is_empty() {
        None
    } else {
        match serde_json::from_slice(&body) {
            Ok(params) => Some(params),
            Err(e) => {
                return error_response(
                    StatusCode::BAD_REQUEST,
                    "validation_error",
                    &format!("Invalid trigger body: {}", e),
                )
                .into_response();
            }
        }
    };

    // Pre-generate run_id so we can return it in the response
    let run_id = Uuid::now_v7();

    // Send the dispatch request to the executor via dispatch channel
    if let Some(ref tx) = state.dispatch_tx {
        let request = DispatchRequest {
            job: job.clone(),
            run_id,
            trigger_params,
        };
        if let Err(e) = tx.send(request).await {
            tracing::warn!("Failed to trigger job '{}': {}", job.name, e);
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({
                    "error": "internal_error",
                    "message": format!("Failed to dispatch job: {}", e),
                })),
            )
                .into_response();
        }
    }

    tracing::info!("Job '{}' triggered (run_id: {})", job.name, run_id);

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "message": "Job triggered",
            "job_id": job.id,
            "job_name": job.name,
            "run_id": run_id,
        })),
    )
        .into_response()
}

/// GET /api/jobs/{id}/runs
pub async fn list_runs(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<ListRunsParams>,
) -> impl IntoResponse {
    let job = match resolve_job(&state, &id).await {
        Ok(j) => j,
        Err(resp) => return resp.into_response(),
    };

    match state
        .log_store
        .list_runs(job.id, params.limit, params.offset)
        .await
    {
        Ok((runs, total)) => {
            // Apply status filter if provided
            let filtered = match params.status {
                Some(ref status_filter) => {
                    let s = status_filter.to_lowercase();
                    runs.into_iter()
                        .filter(|r| {
                            let rs = format!("{:?}", r.status).to_lowercase();
                            rs == s
                        })
                        .collect::<Vec<_>>()
                }
                None => runs,
            };

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "runs": filtered,
                    "total": total,
                    "limit": params.limit,
                    "offset": params.offset,
                })),
            )
                .into_response()
        }
        Err(e) => error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to list runs: {}", e),
        )
        .into_response(),
    }
}

/// GET /api/runs/{run_id}/log
pub async fn get_log(
    State(state): State<Arc<AppState>>,
    Path(run_id_str): Path<String>,
    Query(params): Query<GetLogParams>,
) -> impl IntoResponse {
    let run_id = match Uuid::parse_str(&run_id_str) {
        Ok(id) => id,
        Err(_) => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "validation_error",
                "Invalid run_id format",
            )
            .into_response();
        }
    };

    // We need to find the job_id for this run_id. Search across all jobs.
    let jobs = match state.job_store.list_jobs().await {
        Ok(j) => j,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to list jobs: {}", e),
            )
            .into_response();
        }
    };

    // Try each job to find the run
    for job in &jobs {
        match state.log_store.read_log(job.id, run_id, params.tail).await {
            Ok(content) if !content.is_empty() => {
                return (
                    StatusCode::OK,
                    [(axum::http::header::CONTENT_TYPE, "text/plain")],
                    content,
                )
                    .into_response();
            }
            _ => continue,
        }
    }

    // Also try with a direct read using a nil job_id approach
    // If no log found in any job, check if there is log data with any job_id
    // In the InMemoryLogStore test case, job_id must match, so we try all.
    // If still not found, return 404
    error_response(
        StatusCode::NOT_FOUND,
        "not_found",
        &format!("Log for run '{}' not found", run_id),
    )
    .into_response()
}

/// POST /api/shutdown
pub async fn shutdown(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::info!("Shutdown requested");

    // Signal shutdown via the shutdown_tx if available
    if let Some(ref tx) = state.shutdown_tx {
        let _ = tx.send(());
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Shutdown initiated",
        })),
    )
}

/// GET /api/logs — read daemon/service logs
pub async fn get_daemon_logs(
    State(state): State<Arc<AppState>>,
    Query(params): Query<GetLogParams>,
) -> impl IntoResponse {
    let data_dir = state
        .config
        .data_dir
        .clone()
        .unwrap_or_else(|| crate::daemon::resolve_data_dir(None));
    let log_path = data_dir.join("daemon.log");

    if !log_path.exists() {
        return (
            StatusCode::OK,
            [(axum::http::header::CONTENT_TYPE, "text/plain")],
            "No daemon logs available yet.\n".to_string(),
        )
            .into_response();
    }

    match tokio::fs::read_to_string(&log_path).await {
        Ok(content) => {
            let output = match params.tail {
                Some(n) => {
                    let lines: Vec<&str> = content.lines().collect();
                    let start = if lines.len() > n { lines.len() - n } else { 0 };
                    lines[start..].join("\n")
                }
                None => content,
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, "text/plain")],
                output,
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!("Failed to read daemon log: {}", e);
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to read daemon logs: {}", e),
            )
            .into_response()
        }
    }
}

/// POST /api/restart — restart the daemon
pub async fn restart(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::info!("Restart requested");

    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                &format!("Failed to determine executable path: {}", e),
            )
            .into_response();
        }
    };

    // Spawn a new daemon process
    let spawn_result = {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            std::process::Command::new(&exe_path)
                .args(["start", "--foreground"])
                .creation_flags(CREATE_NO_WINDOW)
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
        }
        #[cfg(not(target_os = "windows"))]
        {
            std::process::Command::new(&exe_path)
                .args(["start", "--foreground"])
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .stdin(std::process::Stdio::null())
                .spawn()
        }
    };

    if let Err(e) = spawn_result {
        return error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            &format!("Failed to spawn new daemon process: {}", e),
        )
        .into_response();
    }

    // Signal shutdown after a short delay to allow the response to be sent
    if let Some(ref tx) = state.shutdown_tx {
        let tx = tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let _ = tx.send(());
        });
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "message": "Restart initiated",
        })),
    )
        .into_response()
}

/// GET /api/service/status
pub async fn service_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
    // Basic service status response
    let platform = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    };

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "platform": platform,
            "service_installed": false,
            "service_running": false,
        })),
    )
}
