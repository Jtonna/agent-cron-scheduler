use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::AppState;

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
    pub active_jobs: usize,
    pub total_jobs: usize,
    pub version: String,
    pub data_dir: String,
}

pub async fn health_check(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::debug!("Health check");

    let uptime = state.start_time.elapsed().as_secs();

    let total_jobs = match state.job_store.list_jobs().await {
        Ok(jobs) => {
            let enabled = jobs.iter().filter(|j| j.enabled).count();
            (enabled, jobs.len())
        }
        Err(_) => (0, 0),
    };

    let data_dir = state
        .config
        .data_dir
        .as_ref()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let response = HealthResponse {
        status: "ok".to_string(),
        uptime_seconds: uptime,
        active_jobs: total_jobs.0,
        total_jobs: total_jobs.1,
        version: "0.1.0".to_string(),
        data_dir,
    };

    (StatusCode::OK, Json(response))
}
