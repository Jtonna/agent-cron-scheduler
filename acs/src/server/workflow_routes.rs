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
use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::AppState;
use crate::daemon::events::{WorkflowChangeKind, WorkflowEvent};
use crate::errors::AcsError;
use crate::models::workflow::{NewWorkflow, RunStatus, TriggerParams, Workflow, WorkflowRun, WorkflowUpdate};
use crate::workflow::{EventEmittingLogSink, FileLogSink};

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
        trigger_input: if params.input.is_null() { None } else { Some(params.input.clone()) },
        steps: vec![],
        total_cost_usd: None,
        total_duration_ms: None,
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
    let kill_signals = Arc::clone(&state.kill_signals);

    // Spawn the workflow run in the background.
    tokio::spawn(async move {
        // Ensure log directory exists.
        if let Err(e) = tokio::fs::create_dir_all(&log_dir).await {
            tracing::error!("Failed to create log dir for workflow run {}: {}", run_id, e);
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

        // Persist the final run state to the store.
        if let Err(e) = run_store.update_run(&final_run).await {
            tracing::error!("Failed to persist final run {}: {}", run_id, e);
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

    match state.workflow_run_store.get_run(run_id).await {
        Ok(Some(run)) => {
            (StatusCode::OK, Json(serde_json::to_value(run).unwrap())).into_response()
        }
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
                tracing::info!(
                    "Kill signal sent to executor for run {}",
                    run_id
                );
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
            StatusCode::ACCEPTED.into_response()
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

    let runs = match state.workflow_run_store.list_runs(workflow.id, limit, offset).await {
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
    use chrono::Utc;
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
                return Err(AcsError::NotFound(format!("Run '{}' not found", run.run_id)));
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
        async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError> {
            self.runs.lock().await.remove(&run_id);
            Ok(())
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
            input_schema: None,
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
        Arc::new(crate::server::AppState {
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(DaemonConfig::default()),
            start_time: Instant::now(),
            shutdown_tx: None,
            workflow_event_tx,
            workflow_store: wf_store,
            workflow_run_store: run_store,
            kill_signals: Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
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
}
