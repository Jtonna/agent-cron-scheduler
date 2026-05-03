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

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::Serialize;
use uuid::Uuid;

use super::AppState;
use crate::daemon::events::{WorkflowChangeKind, WorkflowEvent};
use crate::errors::AcsError;
use crate::models::workflow::{NewWorkflow, RunStatus, TriggerParams, Workflow, WorkflowRun, WorkflowUpdate};
use crate::workflow::FileLogSink;

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
            AcsError::Validation(msg) => (
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
// GET /api/workflows
// ---------------------------------------------------------------------------

pub async fn list_workflows(
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    match state.workflow_store.list_workflows().await {
        Ok(workflows) => (StatusCode::OK, Json(serde_json::to_value(workflows).unwrap())).into_response(),
        Err(e) => {
            tracing::error!("Failed to list workflows: {}", e);
            error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal_error", &e.to_string()).into_response()
        }
    }
}

// ---------------------------------------------------------------------------
// POST /api/workflows
// ---------------------------------------------------------------------------

pub async fn create_workflow(
    State(state): State<Arc<AppState>>,
    Json(body): Json<NewWorkflow>,
) -> impl IntoResponse {
    match state.workflow_store.create_workflow(body).await {
        Ok(wf) => {
            // Broadcast WorkflowChanged{Created}
            let _ = state.workflow_event_tx.send(WorkflowEvent::WorkflowChanged {
                workflow_id: wf.id,
                version: wf.version,
                change_kind: WorkflowChangeKind::Created,
            });
            (StatusCode::CREATED, Json(serde_json::to_value(&wf).unwrap())).into_response()
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
// PATCH /api/workflows/{id}
// ---------------------------------------------------------------------------

pub async fn update_workflow(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<WorkflowUpdate>,
) -> impl IntoResponse {
    let wf = match resolve_workflow(&state, &id).await {
        Ok(w) => w,
        Err((status, body)) => return (status, body).into_response(),
    };

    match state.workflow_store.update_workflow(wf.id, body).await {
        Ok(updated) => {
            // Broadcast WorkflowChanged{Updated}
            let _ = state.workflow_event_tx.send(WorkflowEvent::WorkflowChanged {
                workflow_id: updated.id,
                version: updated.version,
                change_kind: WorkflowChangeKind::Updated,
            });
            (StatusCode::OK, Json(serde_json::to_value(&updated).unwrap())).into_response()
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

    match state.workflow_store.delete_workflow(workflow_id).await {
        Ok(()) => {
            // Broadcast WorkflowChanged{Deleted}
            let _ = state.workflow_event_tx.send(WorkflowEvent::WorkflowChanged {
                workflow_id,
                version,
                change_kind: WorkflowChangeKind::Deleted,
            });
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

    let run_id = Uuid::now_v7();
    let workflow_id = workflow.id;
    let workflow_version = workflow.version;

    // Resolve the data_dir for the log file.
    let data_dir = if let Some(ref dir) = state.config.data_dir {
        std::path::PathBuf::from(dir)
    } else {
        crate::daemon::resolve_data_dir(None)
    };

    let log_dir = data_dir.join("logs").join(workflow_id.to_string());
    let log_path = log_dir.join(format!("{}.log", run_id));

    // Pre-insert a Running WorkflowRun into the in-memory map.
    let initial_run = Arc::new(tokio::sync::RwLock::new(WorkflowRun {
        run_id,
        workflow_id,
        workflow_version,
        workflow_snapshot: workflow.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: RunStatus::Running,
        trigger_input: if params.input.is_null() { None } else { Some(params.input.clone()) },
        steps: vec![],
        total_cost_usd: None,
        total_duration_ms: None,
    }));
    state.workflow_runs.write().await.insert(run_id, Arc::clone(&initial_run));

    let event_tx = state.workflow_event_tx.clone();
    let run_map = Arc::clone(&state.workflow_runs);

    // Spawn the workflow run in the background.
    tokio::spawn(async move {
        // Ensure log directory exists.
        if let Err(e) = tokio::fs::create_dir_all(&log_dir).await {
            tracing::error!("Failed to create log dir for workflow run {}: {}", run_id, e);
            return;
        }

        // Create file log sink.
        let log_sink = match FileLogSink::create(log_path).await {
            Ok(sink) => Arc::new(sink) as Arc<dyn crate::workflow::LogSink>,
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
        )
        .await;

        // Update the in-memory run map with the final result.
        if let Some(entry) = run_map.read().await.get(&run_id) {
            let mut w = entry.write().await;
            *w = final_run;
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(TriggerResponse {
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

    let runs = state.workflow_runs.read().await;
    match runs.get(&run_id) {
        Some(run_lock) => {
            let run = run_lock.read().await.clone();
            (StatusCode::OK, Json(serde_json::to_value(run).unwrap())).into_response()
        }
        None => error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("Run '{}' not found", run_id),
        )
        .into_response(),
    }
}

// ---------------------------------------------------------------------------
// POST /api/runs/{run_id}/kill
// ---------------------------------------------------------------------------
//
// Phase 5 limitation: this sets a "kill flag" by marking the run as Killed
// in the in-memory tracker. Actual process-tree kill wiring (SIGKILL / job
// object termination) is deferred to phase 6, which will add per-run kill
// channels similar to the JobRun executor's kill_tx pattern.

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

    let runs = state.workflow_runs.read().await;
    match runs.get(&run_id) {
        Some(run_lock) => {
            let mut run = run_lock.write().await;
            if run.status == RunStatus::Running {
                // Phase 5: set a kill flag in-memory; actual process kill wired in phase 6.
                tracing::info!(
                    "Kill requested for workflow run {} (phase 5: in-memory flag only; \
                     actual process kill wiring deferred to phase 6)",
                    run_id
                );
                run.status = RunStatus::Killed;
                run.finished_at = Some(Utc::now());
            }
            StatusCode::ACCEPTED.into_response()
        }
        None => error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            &format!("Run '{}' not found", run_id),
        )
        .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use crate::daemon::events::{JobEvent, WorkflowEvent};
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, NewWorkflow, ShellStep, StepDef, StepDefCommon, WorkflowUpdate,
    };
    use crate::models::DaemonConfig;
    use crate::storage::workflows::WorkflowStore;
    use crate::storage::{JobStore, LogStore};
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use crate::models::workflow::Workflow;
    use crate::models::{Job, JobManifest, JobRun, JobUpdate, NewJob};
    use http_body_util::BodyExt;
    use std::collections::HashMap;
    use std::sync::Arc;
    use std::time::Instant;
    use tokio::sync::{broadcast, Notify, RwLock};
    use tower::ServiceExt;
    use uuid::Uuid;
    use chrono::Utc;

    // ── Minimal test stores ──────────────────────────────────────────────────

    struct NoOpJobStore;

    #[async_trait]
    impl JobStore for NoOpJobStore {
        async fn list_jobs(&self) -> anyhow::Result<Vec<Job>> { Ok(vec![]) }
        async fn get_job(&self, _id: Uuid) -> anyhow::Result<Option<Job>> { Ok(None) }
        async fn find_by_name(&self, _name: &str) -> anyhow::Result<Option<Job>> { Ok(None) }
        async fn create_job(&self, _new: NewJob) -> anyhow::Result<Job> { unimplemented!() }
        async fn update_job(&self, _id: Uuid, _update: JobUpdate) -> anyhow::Result<Job> { unimplemented!() }
        async fn delete_job(&self, _id: Uuid) -> anyhow::Result<()> { unimplemented!() }
    }

    struct NoOpLogStore;

    #[async_trait]
    impl LogStore for NoOpLogStore {
        async fn create_run(&self, _run: &JobRun) -> anyhow::Result<()> { Ok(()) }
        async fn update_run(&self, _run: &JobRun) -> anyhow::Result<()> { Ok(()) }
        async fn append_log(&self, _j: Uuid, _r: Uuid, _d: &[u8]) -> anyhow::Result<()> { Ok(()) }
        async fn read_log(&self, _j: Uuid, _r: Uuid, _t: Option<usize>) -> anyhow::Result<String> { Ok(String::new()) }
        async fn list_runs(&self, _j: Uuid, _l: usize, _o: usize) -> anyhow::Result<(Vec<JobRun>, usize)> { Ok((vec![], 0)) }
        async fn cleanup(&self, _j: Uuid, _m: usize) -> anyhow::Result<()> { Ok(()) }
        async fn read_manifest(&self, _j: Uuid) -> anyhow::Result<Option<JobManifest>> { Ok(None) }
        async fn update_manifest(&self, _j: Uuid, _r: &JobRun) -> anyhow::Result<()> { Ok(()) }
        async fn rebuild_manifest(&self, _j: Uuid) -> anyhow::Result<JobManifest> { Ok(JobManifest::new(_j)) }
    }

    struct InMemoryWorkflowStore {
        workflows: RwLock<Vec<crate::models::workflow::Workflow>>,
    }

    impl InMemoryWorkflowStore {
        fn new() -> Self {
            Self { workflows: RwLock::new(vec![]) }
        }
    }

    #[async_trait]
    impl WorkflowStore for InMemoryWorkflowStore {
        async fn list_workflows(&self) -> anyhow::Result<Vec<Workflow>> {
            Ok(self.workflows.read().await.clone())
        }
        async fn get_workflow(&self, id: Uuid) -> anyhow::Result<Option<Workflow>> {
            Ok(self.workflows.read().await.iter().find(|w| w.id == id).cloned())
        }
        async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Workflow>> {
            Ok(self.workflows.read().await.iter().find(|w| w.name == name).cloned())
        }
        async fn create_workflow(&self, new: NewWorkflow) -> anyhow::Result<Workflow> {
            let mut wfs = self.workflows.write().await;
            if wfs.iter().any(|w| w.name == new.name) {
                return Err(crate::errors::AcsError::Conflict(format!(
                    "A workflow with name '{}' already exists", new.name
                )).into());
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
                input_schema: new.input_schema,
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
        async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> anyhow::Result<Workflow> {
            let mut wfs = self.workflows.write().await;
            let idx = wfs.iter().position(|w| w.id == id)
                .ok_or_else(|| anyhow::anyhow!("not found"))?;
            // Check for duplicate name
            if let Some(ref new_name) = update.name {
                if wfs.iter().any(|w| w.name == *new_name && w.id != id) {
                    return Err(crate::errors::AcsError::Conflict(format!(
                        "A workflow with name '{}' already exists", new_name
                    )).into());
                }
            }
            let wf = &mut wfs[idx];
            let mut bumped = false;
            if let Some(n) = update.name { if wf.name != n { bumped = true; } wf.name = n; }
            if let Some(s) = update.schedule { if wf.schedule != s { bumped = true; } wf.schedule = s; }
            if let Some(e) = update.enabled { wf.enabled = e; }
            if let Some(steps) = update.steps { if wf.steps != steps { bumped = true; } wf.steps = steps; }
            if bumped { wf.version += 1; }
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
    }

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
            input_schema: None,
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: None,
            on_failure: FailurePolicy::default(),
        }
    }

    fn make_state(wf_store: Arc<dyn WorkflowStore>) -> Arc<crate::server::AppState> {
        let (event_tx, _) = broadcast::channel::<JobEvent>(256);
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(256);
        Arc::new(crate::server::AppState {
            job_store: Arc::new(NoOpJobStore),
            log_store: Arc::new(NoOpLogStore),
            event_tx,
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(DaemonConfig::default()),
            start_time: Instant::now(),
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            dispatch_tx: None,
            workflow_event_tx,
            workflow_store: wf_store,
            workflow_runs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    async fn body_json(body: Body) -> serde_json::Value {
        let bytes = body.collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_workflows_empty() {
        let state = make_state(Arc::new(InMemoryWorkflowStore::new()));
        let app = crate::server::create_router(state);
        let resp = app
            .oneshot(Request::builder().uri("/api/workflows").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let json = body_json(resp.into_body()).await;
        assert!(json.as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_create_workflow_returns_201() {
        let state = make_state(Arc::new(InMemoryWorkflowStore::new()));
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
    async fn test_create_workflow_duplicate_name_returns_409() {
        let store = Arc::new(InMemoryWorkflowStore::new());
        let state = make_state(Arc::clone(&store) as Arc<dyn WorkflowStore>);
        // Create once
        store.create_workflow(make_new_workflow("dup-wf")).await.unwrap();
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
        let wf = store.create_workflow(make_new_workflow("get-by-id")).await.unwrap();
        let state = make_state(Arc::clone(&store) as Arc<dyn WorkflowStore>);
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
        let _wf = store.create_workflow(make_new_workflow("by-name-wf")).await.unwrap();
        let state = make_state(Arc::clone(&store) as Arc<dyn WorkflowStore>);
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
        let state = make_state(Arc::new(InMemoryWorkflowStore::new()));
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
        let wf = store.create_workflow(make_new_workflow("to-delete")).await.unwrap();
        let state = make_state(Arc::clone(&store) as Arc<dyn WorkflowStore>);
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
}
