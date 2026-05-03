pub mod assets;
pub mod health;
pub mod routes;
pub mod sse;
pub mod workflow_routes;

use std::sync::Arc;
use std::time::Instant;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::{broadcast, Notify};
use tower_http::cors::{Any, CorsLayer};

use crate::daemon::events::WorkflowEvent;
use crate::models::DaemonConfig;
use crate::storage::workflow_runs::WorkflowRunStore;
use crate::storage::workflows::WorkflowStore;

/// Shared application state for the Axum server.
pub struct AppState {
    pub scheduler_notify: Arc<Notify>,
    pub config: Arc<DaemonConfig>,
    pub start_time: Instant,
    pub shutdown_tx: Option<tokio::sync::watch::Sender<()>>,
    /// Broadcast sender for WorkflowEvent SSE stream.
    pub workflow_event_tx: broadcast::Sender<WorkflowEvent>,
    /// Workflow definition store.
    pub workflow_store: Arc<dyn WorkflowStore>,
    /// Persistent workflow run store.
    pub workflow_run_store: Arc<dyn WorkflowRunStore>,
}

/// Create the Axum router with all routes.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        // ── Legacy /api/jobs routes removed in Phase 6 cutover ────────────────
        // ── Workflow routes ───────────────────────────────────────────────────
        .route(
            "/api/workflows",
            get(workflow_routes::list_workflows).post(workflow_routes::create_workflow),
        )
        .route(
            "/api/workflows/{id}",
            get(workflow_routes::get_workflow)
                .patch(workflow_routes::update_workflow)
                .delete(workflow_routes::delete_workflow),
        )
        .route(
            "/api/workflows/{id}/trigger",
            post(workflow_routes::trigger_workflow),
        )
        .route(
            "/api/runs/{run_id}",
            get(workflow_routes::get_workflow_run),
        )
        .route(
            "/api/runs/{run_id}/kill",
            post(workflow_routes::kill_workflow_run),
        )
        // SSE for WorkflowEvent.
        .route(
            "/api/events/workflows",
            get(sse::workflow_events_handler),
        )
        .route("/api/shutdown", post(routes::shutdown))
        .route("/api/restart", post(routes::restart))
        .route("/api/logs", get(routes::get_daemon_logs))
        .route("/api/service/status", get(routes::service_status))
        .with_state(state)
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods(Any)
                .allow_headers(Any),
        )
        .fallback(assets::serve_embedded)
}
// Phase 6: server::tests module removed — all tests covered the old /api/jobs/*
// routes that were deleted in Phase 6 cutover. Workflow API is tested in
// tests/workflow_api_tests.rs.
