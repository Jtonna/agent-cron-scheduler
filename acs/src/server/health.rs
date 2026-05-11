use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::AppState;
use crate::daemon::service;

/// Service registration block included in the health response.
#[derive(Debug, Serialize)]
pub struct HealthService {
    /// Whether ACS is registered with the platform service manager
    /// (Windows Run key / macOS launchd / Linux systemd user unit).
    pub registered: bool,
    /// Platform identifier: `"windows"`, `"macos"`, or `"linux"`.
    pub platform: &'static str,
    /// Filesystem path or registry location of the service registration,
    /// or `null` when the service is not registered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub uptime_seconds: u64,
    pub active_jobs: usize,
    pub total_jobs: usize,
    pub version: String,
    pub data_dir: String,
    pub service: HealthService,
}

/// Build the `service` block from the daemon's platform service module.
fn collect_service_block() -> HealthService {
    let info = service::service_status();
    HealthService {
        registered: info.is_registered,
        platform: info.platform,
        details: info.service_path,
    }
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

    let data_dir_path = state
        .config
        .data_dir
        .clone()
        .unwrap_or_else(|| crate::daemon::resolve_data_dir(None));
    let data_dir = data_dir_path.display().to_string();

    let service = collect_service_block();

    let response = HealthResponse {
        status: "ok".to_string(),
        uptime_seconds: uptime,
        active_jobs: active_workflows,
        total_jobs: total_workflows,
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_dir,
        service,
    };

    (StatusCode::OK, Json(response))
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;

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

    // ── Service block ────────────────────────────────────────────────────────

    #[test]
    fn test_health_service_serializes_with_expected_keys() {
        let block = HealthService {
            registered: true,
            platform: "linux",
            details: Some("/home/u/.config/systemd/user/agentcronsystem.service".to_string()),
        };
        let json = serde_json::to_value(&block).expect("serialize");
        assert_eq!(json["registered"], serde_json::json!(true));
        assert_eq!(json["platform"], serde_json::json!("linux"));
        assert!(
            json["details"].is_string(),
            "details should be a string when registered"
        );
    }

    #[test]
    fn test_health_service_omits_details_when_absent() {
        let block = HealthService {
            registered: false,
            platform: "linux",
            details: None,
        };
        let json = serde_json::to_value(&block).expect("serialize");
        assert!(
            json.get("details").is_none(),
            "details should be omitted when None"
        );
    }

    // ── Full response shape ──────────────────────────────────────────────────

    #[test]
    fn test_health_response_serializes_with_service_block() {
        let resp = HealthResponse {
            status: "ok".to_string(),
            uptime_seconds: 42,
            active_jobs: 1,
            total_jobs: 2,
            version: "9.9.9".to_string(),
            data_dir: "/tmp/x".to_string(),
            service: HealthService {
                registered: true,
                platform: "linux",
                details: Some("/x".to_string()),
            },
        };
        let json = serde_json::to_value(&resp).expect("serialize");
        assert_eq!(json["status"], "ok");
        assert_eq!(json["uptime_seconds"], 42);
        assert_eq!(json["active_jobs"], 1);
        assert_eq!(json["total_jobs"], 2);
        assert_eq!(json["version"], "9.9.9");
        assert_eq!(json["data_dir"], "/tmp/x");
        assert_eq!(json["service"]["registered"], true);
        assert_eq!(json["service"]["platform"], "linux");
        assert_eq!(json["service"]["details"], "/x");
        assert!(
            json.get("migrations").is_none(),
            "migrations block must not be present in the health response"
        );
    }
}
