//! Server utility routes: shutdown, restart, daemon logs, service status.
//!
//! The old /api/jobs/* handlers that lived here were removed in Phase 6
//! cutover. Only the four utility handlers are still registered.

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use super::AppState;

// ---------------------------------------------------------------------------
// Error response (local helper)
// ---------------------------------------------------------------------------

fn error_response(
    status: StatusCode,
    error: &str,
    message: &str,
) -> impl IntoResponse {
    (
        status,
        Json(serde_json::json!({
            "error": error,
            "message": message,
        })),
    )
}

// ---------------------------------------------------------------------------
// POST /api/shutdown
// ---------------------------------------------------------------------------

pub async fn shutdown(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::info!("Shutdown requested");

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

// ---------------------------------------------------------------------------
// GET /api/logs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
pub struct GetLogParams {
    pub tail: Option<usize>,
}

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

// ---------------------------------------------------------------------------
// POST /api/restart
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// GET /api/service/status
// ---------------------------------------------------------------------------

pub async fn service_status(State(_state): State<Arc<AppState>>) -> impl IntoResponse {
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
