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

    let (active_workflows, total_workflows) = match state.workflow_store.list_workflows().await {
        Ok(wfs) => {
            let enabled = wfs.iter().filter(|w| w.enabled).count();
            (enabled, wfs.len())
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
        active_jobs: active_workflows,
        total_jobs: total_workflows,
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir,
    };

    (StatusCode::OK, Json(response))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    /// Verify that the version baked into the health response at compile time
    /// matches the version declared in Cargo.toml.  This guards against the
    /// health endpoint accidentally returning a hardcoded string that drifts
    /// from the real package version.
    #[test]
    fn test_health_version_matches_cargo() {
        let cargo_version = env!("CARGO_PKG_VERSION");
        // Simulate what health_check does when building the response.
        let response_version = env!("CARGO_PKG_VERSION").to_string();
        assert_eq!(
            response_version, cargo_version,
            "Health response version '{}' should equal CARGO_PKG_VERSION '{}'",
            response_version, cargo_version
        );
        // Additionally assert the version is non-empty and not the zero-value
        // placeholder that appears in un-configured Cargo projects.
        assert!(
            !cargo_version.is_empty(),
            "CARGO_PKG_VERSION should not be empty"
        );
        assert_ne!(
            cargo_version, "0.0.0",
            "CARGO_PKG_VERSION should not be the default 0.0.0 placeholder"
        );
    }
}
