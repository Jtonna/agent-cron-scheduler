pub mod assets;
pub mod health;
pub mod routes;
pub mod sse;
pub mod timeframe;
pub mod workflow_routes;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::{broadcast, Notify, RwLock};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::daemon::events::{JobEvent, WorkflowEvent};
use crate::daemon::executor::RunHandle;
use crate::models::workflow::WorkflowRun;
use crate::models::DaemonConfig;
use crate::storage::{JobStore, LogStore};
use crate::storage::workflows::WorkflowStore;

/// Shared application state for the Axum server.
pub struct AppState {
    pub job_store: Arc<dyn JobStore>,
    pub log_store: Arc<dyn LogStore>,
    pub event_tx: broadcast::Sender<JobEvent>,
    pub scheduler_notify: Arc<Notify>,
    pub config: Arc<DaemonConfig>,
    pub start_time: Instant,
    pub active_runs: Arc<RwLock<HashMap<Uuid, Vec<RunHandle>>>>,
    pub shutdown_tx: Option<tokio::sync::watch::Sender<()>>,
    pub dispatch_tx: Option<tokio::sync::mpsc::Sender<crate::models::DispatchRequest>>,
    /// Broadcast sender for WorkflowEvent SSE stream.
    pub workflow_event_tx: broadcast::Sender<WorkflowEvent>,
    /// Workflow definition store.
    pub workflow_store: Arc<dyn WorkflowStore>,
    /// In-memory map of run_id → WorkflowRun (phase 5: in-memory only).
    /// Phase 6 will persist runs to disk.
    pub workflow_runs: Arc<RwLock<HashMap<Uuid, Arc<RwLock<WorkflowRun>>>>>,
}

/// Create the Axum router with all routes.
pub fn create_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/health", get(health::health_check))
        .route("/api/jobs", get(routes::list_jobs).post(routes::create_job))
        .route(
            "/api/jobs/{id}",
            get(routes::get_job)
                .patch(routes::update_job)
                .delete(routes::delete_job),
        )
        .route("/api/jobs/{id}/enable", post(routes::enable_job))
        .route("/api/jobs/{id}/disable", post(routes::disable_job))
        .route("/api/jobs/{id}/trigger", post(routes::trigger_job))
        .route("/api/jobs/{id}/kill", post(routes::kill_job))
        .route("/api/jobs/{id}/runs", get(routes::list_runs))
        .route("/api/jobs/{id}/manifest", get(routes::get_job_manifest))
        .route(
            "/api/jobs/{id}/cost-summary",
            get(routes::get_job_cost_summary),
        )
        .route("/api/costs/summary", get(routes::get_global_cost_summary))
        .route("/api/runs/recent", get(routes::list_recent_runs))
        .route("/api/runs/{run_id}/log", get(routes::get_log))
        .route("/api/events", get(sse::sse_handler))
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
        // SSE for WorkflowEvent — mounted at /api/events/workflows to avoid
        // clashing with the existing /api/events (JobEvent SSE).
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

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::{JobEvent, WorkflowEvent};
    use crate::models::job::{ExecutionType, NewJob};
    use crate::models::workflow::{NewWorkflow, Workflow, WorkflowUpdate};
    use crate::models::{Job, JobRun, JobUpdate, RunStatus};
    use crate::storage::workflows::WorkflowStore;
    use crate::storage::{JobStore, LogStore};
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    // -----------------------------------------------------------------------
    // InMemoryWorkflowStore - minimal test double for server/mod.rs tests
    // -----------------------------------------------------------------------

    struct InMemoryWorkflowStore;

    #[async_trait]
    impl WorkflowStore for InMemoryWorkflowStore {
        async fn list_workflows(&self) -> anyhow::Result<Vec<Workflow>> {
            Ok(vec![])
        }
        async fn get_workflow(&self, _id: Uuid) -> anyhow::Result<Option<Workflow>> {
            Ok(None)
        }
        async fn find_by_name(&self, _name: &str) -> anyhow::Result<Option<Workflow>> {
            Ok(None)
        }
        async fn create_workflow(&self, _new: NewWorkflow) -> anyhow::Result<Workflow> {
            Err(anyhow::anyhow!("not implemented in test double"))
        }
        async fn update_workflow(&self, _id: Uuid, _update: WorkflowUpdate) -> anyhow::Result<Workflow> {
            Err(anyhow::anyhow!("not implemented in test double"))
        }
        async fn delete_workflow(&self, _id: Uuid) -> anyhow::Result<()> {
            Err(anyhow::anyhow!("not implemented in test double"))
        }
    }

    // -----------------------------------------------------------------------
    // InMemoryJobStore - test double
    // -----------------------------------------------------------------------

    struct InMemoryJobStore {
        jobs: RwLock<Vec<Job>>,
    }

    impl InMemoryJobStore {
        fn new() -> Self {
            Self {
                jobs: RwLock::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl JobStore for InMemoryJobStore {
        async fn list_jobs(&self) -> anyhow::Result<Vec<Job>> {
            Ok(self.jobs.read().await.clone())
        }

        async fn get_job(&self, id: Uuid) -> anyhow::Result<Option<Job>> {
            Ok(self.jobs.read().await.iter().find(|j| j.id == id).cloned())
        }

        async fn find_by_name(&self, name: &str) -> anyhow::Result<Option<Job>> {
            Ok(self
                .jobs
                .read()
                .await
                .iter()
                .find(|j| j.name == name)
                .cloned())
        }

        async fn create_job(&self, new: NewJob) -> anyhow::Result<Job> {
            let mut jobs = self.jobs.write().await;
            // Check duplicate
            if jobs.iter().any(|j| j.name == new.name) {
                return Err(anyhow::anyhow!(
                    "Conflict: A job with name '{}' already exists",
                    new.name
                ));
            }
            let now = Utc::now();
            let job = Job {
                id: Uuid::now_v7(),
                name: new.name,
                schedule: new.schedule,
                execution: new.execution,
                enabled: new.enabled,
                timezone: new.timezone,
                working_dir: new.working_dir,
                env_vars: new.env_vars,
                timeout_secs: new.timeout_secs,
                log_environment: new.log_environment,
                pre_hook: new.pre_hook,
                post_hook: new.post_hook,
                pre_hook_script_type: new.pre_hook_script_type,
                post_hook_script_type: new.post_hook_script_type,
                allow_concurrent: new.allow_concurrent.unwrap_or(false),
                schedule_mode: new.schedule_mode.unwrap_or_default(),
                created_at: now,
                updated_at: now,
                last_run_at: None,
                last_exit_code: None,
                next_run_at: None,
            };
            jobs.push(job.clone());
            Ok(job)
        }

        async fn update_job(&self, id: Uuid, update: JobUpdate) -> anyhow::Result<Job> {
            let mut jobs = self.jobs.write().await;
            let job = jobs
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or_else(|| anyhow::anyhow!("not found"))?;
            if let Some(name) = update.name {
                job.name = name;
            }
            if let Some(schedule) = update.schedule {
                job.schedule = schedule;
            }
            if let Some(execution) = update.execution {
                job.execution = execution;
            }
            if let Some(enabled) = update.enabled {
                job.enabled = enabled;
            }
            if let Some(tz) = update.timezone {
                job.timezone = Some(tz);
            }
            if let Some(wd) = update.working_dir {
                job.working_dir = Some(wd);
            }
            if let Some(ev) = update.env_vars {
                job.env_vars = Some(ev);
            }
            if let Some(ts) = update.timeout_secs {
                job.timeout_secs = ts;
            }
            job.updated_at = Utc::now();
            Ok(job.clone())
        }

        async fn delete_job(&self, id: Uuid) -> anyhow::Result<()> {
            let mut jobs = self.jobs.write().await;
            jobs.retain(|j| j.id != id);
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // InMemoryLogStore - test double
    // -----------------------------------------------------------------------

    struct InMemoryLogStore {
        runs: RwLock<Vec<JobRun>>,
        logs: RwLock<HashMap<(Uuid, Uuid), Vec<u8>>>,
        manifests: std::sync::RwLock<HashMap<Uuid, crate::models::JobManifest>>,
    }

    impl InMemoryLogStore {
        fn new() -> Self {
            Self {
                runs: RwLock::new(Vec::new()),
                logs: RwLock::new(HashMap::new()),
                manifests: std::sync::RwLock::new(HashMap::new()),
            }
        }

        pub fn seed_manifest(&self, job_id: Uuid, manifest: crate::models::JobManifest) {
            self.manifests.write().unwrap().insert(job_id, manifest);
        }
    }

    #[async_trait]
    impl LogStore for InMemoryLogStore {
        async fn create_run(&self, run: &JobRun) -> anyhow::Result<()> {
            self.runs.write().await.push(run.clone());
            Ok(())
        }

        async fn update_run(&self, run: &JobRun) -> anyhow::Result<()> {
            let mut runs = self.runs.write().await;
            if let Some(existing) = runs.iter_mut().find(|r| r.run_id == run.run_id) {
                *existing = run.clone();
            }
            Ok(())
        }

        async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> anyhow::Result<()> {
            let mut logs = self.logs.write().await;
            let entry = logs.entry((job_id, run_id)).or_default();
            entry.extend_from_slice(data);
            Ok(())
        }

        async fn read_log(
            &self,
            job_id: Uuid,
            run_id: Uuid,
            tail: Option<usize>,
        ) -> anyhow::Result<String> {
            let logs = self.logs.read().await;
            match logs.get(&(job_id, run_id)) {
                Some(data) => {
                    let full = String::from_utf8_lossy(data).to_string();
                    match tail {
                        Some(n) => {
                            let lines: Vec<&str> = full.lines().collect();
                            let start = if lines.len() > n { lines.len() - n } else { 0 };
                            Ok(lines[start..].join("\n"))
                        }
                        None => Ok(full),
                    }
                }
                None => Ok(String::new()),
            }
        }

        async fn list_runs(
            &self,
            job_id: Uuid,
            limit: usize,
            offset: usize,
        ) -> anyhow::Result<(Vec<JobRun>, usize)> {
            let runs = self.runs.read().await;
            let filtered: Vec<JobRun> = runs
                .iter()
                .filter(|r| r.job_id == job_id)
                .cloned()
                .collect();
            let total = filtered.len();
            let paginated = filtered.into_iter().skip(offset).take(limit).collect();
            Ok((paginated, total))
        }

        async fn cleanup(&self, _job_id: Uuid, _max_files: usize) -> anyhow::Result<()> {
            Ok(())
        }

        async fn read_manifest(
            &self,
            job_id: Uuid,
        ) -> anyhow::Result<Option<crate::models::JobManifest>> {
            Ok(self.manifests.read().unwrap().get(&job_id).cloned())
        }

        async fn update_manifest(&self, job_id: Uuid, run: &JobRun) -> anyhow::Result<()> {
            let mut manifests = self.manifests.write().unwrap();
            let manifest = manifests
                .entry(job_id)
                .or_insert_with(|| crate::models::JobManifest::new(job_id));
            manifest.merge_run(run);
            Ok(())
        }

        async fn rebuild_manifest(
            &self,
            job_id: Uuid,
        ) -> anyhow::Result<crate::models::JobManifest> {
            Ok(crate::models::JobManifest::new(job_id))
        }
    }

    // -----------------------------------------------------------------------
    // Test helper: build AppState and Router
    // -----------------------------------------------------------------------

    fn make_test_state() -> Arc<AppState> {
        let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(4096);
        Arc::new(AppState {
            job_store: Arc::new(InMemoryJobStore::new()),
            log_store: Arc::new(InMemoryLogStore::new()),
            event_tx,
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(DaemonConfig::default()),
            start_time: Instant::now(),
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            dispatch_tx: None,
            workflow_event_tx,
            workflow_store: Arc::new(InMemoryWorkflowStore),
            workflow_runs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn make_test_state_with_stores(
        job_store: Arc<dyn JobStore>,
        log_store: Arc<dyn LogStore>,
    ) -> Arc<AppState> {
        let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(4096);
        Arc::new(AppState {
            job_store,
            log_store,
            event_tx,
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(DaemonConfig::default()),
            start_time: Instant::now(),
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            dispatch_tx: None,
            workflow_event_tx,
            workflow_store: Arc::new(InMemoryWorkflowStore),
            workflow_runs: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    fn make_test_app(state: Arc<AppState>) -> Router {
        create_router(state)
    }

    /// Helper to read the full body from a response.
    async fn body_string(body: Body) -> String {
        let bytes = body.collect().await.unwrap().to_bytes();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    fn new_job_json(name: &str) -> String {
        serde_json::json!({
            "name": name,
            "schedule": "*/5 * * * *",
            "execution": {
                "type": "ShellCommand",
                "value": "echo hello"
            }
        })
        .to_string()
    }

    // =======================================================================
    // 1. GET /health returns 200 with all expected fields
    // =======================================================================
    #[tokio::test]
    async fn test_health_returns_200_with_expected_fields() {
        let state = make_test_state();
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["status"], "ok");
        assert!(json["uptime_seconds"].is_number());
        assert!(json["active_jobs"].is_number());
        assert!(json["total_jobs"].is_number());
        assert!(json["version"].is_string());
    }

    // =======================================================================
    // 2. POST /api/jobs with valid body returns 201
    // =======================================================================
    #[tokio::test]
    async fn test_create_job_valid_returns_201() {
        let state = make_test_state();
        let app = make_test_app(state);

        let body = new_job_json("test-job");
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CREATED);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["name"], "test-job");
        assert_eq!(json["schedule"], "*/5 * * * *");
        assert!(json["id"].is_string());
        assert_eq!(json["enabled"], true);
    }

    // =======================================================================
    // 3. POST /api/jobs with invalid cron returns 400
    // =======================================================================
    #[tokio::test]
    async fn test_create_job_invalid_cron_returns_400() {
        let state = make_test_state();
        let app = make_test_app(state);

        let body = serde_json::json!({
            "name": "bad-cron",
            "schedule": "not a cron",
            "execution": {
                "type": "ShellCommand",
                "value": "echo hello"
            }
        })
        .to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json["error"].is_string());
        assert!(json["message"].is_string());
    }

    // =======================================================================
    // 4. POST /api/jobs with duplicate name returns 409
    // =======================================================================
    #[tokio::test]
    async fn test_create_job_duplicate_name_returns_409() {
        let state = make_test_state();

        // Create the first job directly
        state
            .job_store
            .create_job(NewJob {
                name: "dup-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo hello".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let body = new_job_json("dup-job");
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json["error"].is_string());
        assert!(json["message"].is_string());
    }

    // =======================================================================
    // 5. POST /api/jobs with UUID-like name returns 400
    // =======================================================================
    #[tokio::test]
    async fn test_create_job_uuid_name_returns_400() {
        let state = make_test_state();
        let app = make_test_app(state);

        let uuid_name = Uuid::now_v7().to_string();
        let body = serde_json::json!({
            "name": uuid_name,
            "schedule": "*/5 * * * *",
            "execution": {
                "type": "ShellCommand",
                "value": "echo hello"
            }
        })
        .to_string();

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json["error"].is_string());
        assert!(json["message"].as_str().unwrap().contains("UUID"));
    }

    // =======================================================================
    // 6. GET /api/jobs returns all jobs
    // =======================================================================
    #[tokio::test]
    async fn test_list_jobs_returns_all() {
        let state = make_test_state();

        // Create a few jobs
        for name in &["job-a", "job-b", "job-c"] {
            state
                .job_store
                .create_job(NewJob {
                    name: name.to_string(),
                    schedule: "*/5 * * * *".to_string(),
                    execution: ExecutionType::ShellCommand("echo".to_string()),
                    enabled: true,
                    timezone: None,
                    working_dir: None,
                    env_vars: None,
                    timeout_secs: 0,
                    log_environment: false,
                    allow_concurrent: None,
                    schedule_mode: None,
                    pre_hook: None,
                    post_hook: None,
                    pre_hook_script_type: None,
                    post_hook_script_type: None,
                })
                .await
                .unwrap();
        }

        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        assert_eq!(json.len(), 3);
    }

    // =======================================================================
    // 7. GET /api/jobs?enabled=true filters correctly
    // =======================================================================
    #[tokio::test]
    async fn test_list_jobs_enabled_filter() {
        let state = make_test_state();

        // Create enabled and disabled jobs
        state
            .job_store
            .create_job(NewJob {
                name: "enabled-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        state
            .job_store
            .create_job(NewJob {
                name: "disabled-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: false,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs?enabled=true")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        assert_eq!(json.len(), 1);
        assert_eq!(json[0]["name"], "enabled-job");
    }

    // =======================================================================
    // 8. GET /api/jobs/{id} returns job by UUID
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_by_uuid() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "fetch-me".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["name"], "fetch-me");
        assert_eq!(json["id"], job.id.to_string());
    }

    // =======================================================================
    // 9. GET /api/jobs/{name} returns job by name
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_by_name() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "my-named-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs/my-named-job")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["name"], "my-named-job");
        assert_eq!(json["id"], job.id.to_string());
    }

    // =======================================================================
    // 10. GET /api/jobs/{id} returns 404 for unknown ID
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_unknown_returns_404() {
        let state = make_test_state();
        let app = make_test_app(state);

        let unknown_uuid = Uuid::now_v7();
        let uri = format!("/api/jobs/{}", unknown_uuid);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["error"], "not_found");
        assert!(json["message"].is_string());
    }

    // =======================================================================
    // 11. PATCH /api/jobs/{id} updates fields
    // =======================================================================
    #[tokio::test]
    async fn test_update_job_fields() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "update-me".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let update_body = serde_json::json!({
            "name": "updated-name",
            "schedule": "0 * * * *"
        })
        .to_string();

        let uri = format!("/api/jobs/{}", job.id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(&uri)
                    .header("content-type", "application/json")
                    .body(Body::from(update_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["name"], "updated-name");
        assert_eq!(json["schedule"], "0 * * * *");
    }

    // =======================================================================
    // 12. PATCH /api/jobs/{id} validates name uniqueness (409 on conflict)
    // =======================================================================
    #[tokio::test]
    async fn test_update_job_name_conflict_returns_409() {
        let state = make_test_state();

        // Create two jobs
        state
            .job_store
            .create_job(NewJob {
                name: "job-a".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let job_b = state
            .job_store
            .create_job(NewJob {
                name: "job-b".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        // Try to rename job-b to job-a (conflict)
        let update_body = serde_json::json!({
            "name": "job-a"
        })
        .to_string();

        let uri = format!("/api/jobs/{}", job_b.id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(&uri)
                    .header("content-type", "application/json")
                    .body(Body::from(update_body))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::CONFLICT);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json["error"].is_string());
        assert!(json["message"].is_string());
    }

    // =======================================================================
    // 13. DELETE /api/jobs/{id} returns 204
    // =======================================================================
    #[tokio::test]
    async fn test_delete_job_returns_204() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "delete-me".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}", job.id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NO_CONTENT);
    }

    // =======================================================================
    // 14. POST /api/jobs/{id}/enable enables job
    // =======================================================================
    #[tokio::test]
    async fn test_enable_job() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "disabled-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: false,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        assert!(!job.enabled);

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/enable", job.id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["enabled"], true);
    }

    // =======================================================================
    // 15. POST /api/jobs/{id}/disable disables job
    // =======================================================================
    #[tokio::test]
    async fn test_disable_job() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "enabled-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        assert!(job.enabled);

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/disable", job.id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["enabled"], false);
    }

    // =======================================================================
    // 16. POST /api/jobs/{id}/trigger returns 202
    // =======================================================================
    #[tokio::test]
    async fn test_trigger_job_returns_202() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(NewJob {
                name: "trigger-me".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/trigger", job.id);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["message"], "Job triggered");
        assert_eq!(json["job_id"], job.id.to_string());
        assert!(json["run_id"].is_string(), "Response should include run_id");
    }

    // =======================================================================
    // 17. GET /api/jobs/{id}/runs with pagination
    // =======================================================================
    #[tokio::test]
    async fn test_list_runs_with_pagination() {
        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        // Create a job
        let job = job_store
            .create_job(NewJob {
                name: "runs-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        // Create some runs
        for _ in 0..5 {
            let run = JobRun {
                run_id: Uuid::now_v7(),
                job_id: job.id,
                started_at: Utc::now(),
                finished_at: Some(Utc::now()),
                status: RunStatus::Completed,
                exit_code: Some(0),
                log_size_bytes: 100,
                error: None,
                trigger_params: None,
                total_cost_usd: None,
                duration_ms: None,
                num_turns: None,
                model: None,
                usage: None,
            };
            log_store.create_run(&run).await.unwrap();
        }

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/runs?limit=2&offset=0", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["total"], 5);
        assert_eq!(json["limit"], 2);
        assert_eq!(json["offset"], 0);
        assert_eq!(json["runs"].as_array().unwrap().len(), 2);
    }

    // =======================================================================
    // 18. GET /api/runs/{run_id}/log returns log text
    // =======================================================================
    #[tokio::test]
    async fn test_get_log_returns_text() {
        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        // Create a job
        let job = job_store
            .create_job(NewJob {
                name: "log-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let run_id = Uuid::now_v7();
        let run = JobRun {
            run_id,
            job_id: job.id,
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            status: RunStatus::Completed,
            exit_code: Some(0),
            log_size_bytes: 0,
            error: None,
            trigger_params: None,
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        };
        log_store.create_run(&run).await.unwrap();

        // Append log data
        log_store
            .append_log(job.id, run_id, b"Hello from the log\nLine 2\n")
            .await
            .unwrap();

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!("/api/runs/{}/log", run_id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        assert!(body.contains("Hello from the log"));
        assert!(body.contains("Line 2"));
    }

    // =======================================================================
    // 19. All error responses match { "error": ..., "message": ... } format
    // =======================================================================
    #[tokio::test]
    async fn test_error_responses_match_format() {
        let state = make_test_state();
        let app = make_test_app(state);

        // 404 for unknown job
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs/nonexistent-name")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Must have both "error" and "message" fields
        assert!(
            json.get("error").is_some(),
            "Error response must have 'error' field"
        );
        assert!(
            json.get("message").is_some(),
            "Error response must have 'message' field"
        );
        assert!(json["error"].is_string());
        assert!(json["message"].is_string());
    }

    // =======================================================================
    // 20. POST /api/shutdown returns 200
    // =======================================================================
    #[tokio::test]
    async fn test_shutdown_returns_200() {
        let state = make_test_state();
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/shutdown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["message"], "Shutdown initiated");
    }

    // =======================================================================
    // Additional: GET /api/service/status returns 200
    // =======================================================================
    #[tokio::test]
    async fn test_service_status_returns_200() {
        let state = make_test_state();
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/service/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert!(json["platform"].is_string());
    }

    // =======================================================================
    // Additional: DELETE /api/jobs/{id} with nonexistent returns 404
    // =======================================================================
    #[tokio::test]
    async fn test_delete_nonexistent_job_returns_404() {
        let state = make_test_state();
        let app = make_test_app(state);

        let unknown_uuid = Uuid::now_v7();
        let uri = format!("/api/jobs/{}", unknown_uuid);
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =======================================================================
    // Additional: GET /api/jobs/{name} by name returns 404 for unknown name
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_unknown_name_returns_404() {
        let state = make_test_state();
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/jobs/i-do-not-exist")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =======================================================================
    // Additional: POST /api/jobs/{id}/trigger for unknown job returns 404
    // =======================================================================
    #[tokio::test]
    async fn test_trigger_unknown_job_returns_404() {
        let state = make_test_state();
        let app = make_test_app(state);

        let unknown = Uuid::now_v7();
        let uri = format!("/api/jobs/{}/trigger", unknown);
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =======================================================================
    // Additional: Verify events are broadcast on create
    // =======================================================================
    #[tokio::test]
    async fn test_create_job_broadcasts_event() {
        let (event_tx, mut event_rx) = broadcast::channel::<JobEvent>(4096);
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(4096);
        let state = Arc::new(AppState {
            job_store: Arc::new(InMemoryJobStore::new()),
            log_store: Arc::new(InMemoryLogStore::new()),
            event_tx,
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(DaemonConfig::default()),
            start_time: Instant::now(),
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            dispatch_tx: None,
            workflow_event_tx,
            workflow_store: Arc::new(InMemoryWorkflowStore),
            workflow_runs: Arc::new(RwLock::new(HashMap::new())),
        });

        let app = make_test_app(state);

        let body = new_job_json("event-test");
        let _response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/jobs")
                    .header("content-type", "application/json")
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();

        // Check that a JobChanged::Added event was broadcast
        let event = event_rx.try_recv();
        assert!(event.is_ok(), "Should have received an event");
        match event.unwrap() {
            JobEvent::JobChanged { change, .. } => {
                match change {
                    crate::daemon::events::JobChangeKind::Added => {} // correct
                    other => panic!("Expected Added, got {:?}", other),
                }
            }
            other => panic!("Expected JobChanged event, got {:?}", other),
        }
    }

    // =======================================================================
    // Additional: GET /api/logs returns daemon logs placeholder
    // =======================================================================
    #[tokio::test]
    async fn test_get_daemon_logs_no_file() {
        // Use a temp dir so we don't pick up a real daemon.log from the system
        let tmp_dir = tempfile::TempDir::new().unwrap();
        let config = DaemonConfig {
            data_dir: Some(tmp_dir.path().to_path_buf()),
            ..Default::default()
        };

        let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
        let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(4096);
        let state = Arc::new(AppState {
            job_store: Arc::new(InMemoryJobStore::new()),
            log_store: Arc::new(InMemoryLogStore::new()),
            event_tx,
            scheduler_notify: Arc::new(Notify::new()),
            config: Arc::new(config),
            start_time: Instant::now(),
            active_runs: Arc::new(RwLock::new(HashMap::new())),
            shutdown_tx: None,
            dispatch_tx: None,
            workflow_event_tx,
            workflow_store: Arc::new(InMemoryWorkflowStore),
            workflow_runs: Arc::new(RwLock::new(HashMap::new())),
        });
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/logs")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        assert!(
            body.contains("No daemon logs available yet"),
            "Should return placeholder when no log file exists, got: {}",
            body
        );
    }

    // =======================================================================
    // Additional: POST /api/restart returns 200
    // =======================================================================
    #[tokio::test]
    async fn test_restart_returns_200() {
        let state = make_test_state();
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/restart")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // Note: restart will fail to spawn because the test binary != acs,
        // so it may return 500. In a real environment it returns 200.
        // We accept either 200 or 500 since both prove the route is registered.
        assert!(
            response.status() == StatusCode::OK
                || response.status() == StatusCode::INTERNAL_SERVER_ERROR,
            "Restart route should be registered, got: {}",
            response.status()
        );
    }

    // =======================================================================
    // Additional: Health shows correct active/total count
    // =======================================================================
    #[tokio::test]
    async fn test_health_shows_correct_job_counts() {
        let state = make_test_state();

        // Create 3 jobs: 2 enabled, 1 disabled
        for (name, enabled) in &[("a", true), ("b", true), ("c", false)] {
            state
                .job_store
                .create_job(NewJob {
                    name: name.to_string(),
                    schedule: "*/5 * * * *".to_string(),
                    execution: ExecutionType::ShellCommand("echo".to_string()),
                    enabled: *enabled,
                    timezone: None,
                    working_dir: None,
                    env_vars: None,
                    timeout_secs: 0,
                    log_environment: false,
                    allow_concurrent: None,
                    schedule_mode: None,
                    pre_hook: None,
                    post_hook: None,
                    pre_hook_script_type: None,
                    post_hook_script_type: None,
                })
                .await
                .unwrap();
        }

        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["total_jobs"], 3);
        assert_eq!(json["active_jobs"], 2);
    }

    // =======================================================================
    // Helper: build a seeded manifest with the given daily bucket dates
    // =======================================================================

    fn make_manifest_with_buckets(
        job_id: uuid::Uuid,
        buckets: &[(&str, u64, f64)], // (date_key, runs, cost_usd)
    ) -> crate::models::manifest::JobManifest {
        use crate::models::manifest::{JobManifest, ModelUsageBucket, TimeBucket};
        let mut manifest = JobManifest::new(job_id);
        manifest.total_runs = buckets.iter().map(|(_, r, _)| r).sum();
        manifest.total_cost_usd = buckets.iter().map(|(_, _, c)| c).sum();
        for (date_key, runs, cost_usd) in buckets {
            let mut bucket = TimeBucket::default();
            bucket.runs = *runs;
            bucket.cost_usd = *cost_usd;
            bucket.duration_ms = runs * 60_000;
            bucket.runs_by_status.insert("Completed".to_string(), *runs);
            let mut model_usage = ModelUsageBucket::default();
            model_usage.input_tokens = runs * 1000;
            model_usage.output_tokens = runs * 250;
            model_usage.cache_read_input_tokens = runs * 500;
            bucket
                .models
                .insert("claude-sonnet".to_string(), model_usage);
            manifest.daily_buckets.insert(date_key.to_string(), bucket);
        }
        manifest
    }

    // Helper: create a job in an InMemoryJobStore
    async fn create_job_in_store(store: &InMemoryJobStore, name: &str) -> crate::models::Job {
        store
            .create_job(crate::models::job::NewJob {
                name: name.to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: crate::models::job::ExecutionType::ShellCommand("echo hi".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap()
    }

    // =======================================================================
    // 20. GET /api/jobs/{id}/cost-summary — manifest present, default timeframe
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_with_manifest() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "cost-job-1").await;

        // Build recent buckets within the default 30-day window
        let today = Utc::now().date_naive();
        let day1 = (today - chrono::Duration::days(5))
            .format("%Y-%m-%d")
            .to_string();
        let day2 = (today - chrono::Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();

        let manifest = make_manifest_with_buckets(
            job.id,
            &[(day1.as_str(), 3, 1.50), (day2.as_str(), 2, 0.80)],
        );
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/cost-summary", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["job_id"], job.id.to_string());
        assert!(json["timeframe"].is_string());

        let summary = &json["summary"];
        assert_eq!(summary["total_runs"], 5);
        assert!((summary["total_cost_usd"].as_f64().unwrap() - 2.30).abs() < 0.001);

        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
    }

    // =======================================================================
    // 21. GET /api/jobs/{id}/cost-summary — no manifest → zeroed response
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_no_manifest() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(crate::models::job::NewJob {
                name: "no-manifest-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: crate::models::job::ExecutionType::ShellCommand("echo hi".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/cost-summary", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let summary = &json["summary"];
        assert_eq!(summary["total_runs"], 0);
        assert_eq!(summary["total_cost_usd"], 0.0);

        let data = json["data"].as_array().unwrap();
        assert!(data.is_empty());
    }

    // =======================================================================
    // 22. GET /api/jobs/{id}/cost-summary — non-existent job → 404
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_not_found() {
        let state = make_test_state();
        let app = make_test_app(state);

        let unknown = uuid::Uuid::now_v7();
        let uri = format!("/api/jobs/{}/cost-summary", unknown);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =======================================================================
    // 23. GET /api/jobs/{id}/cost-summary?timeframe=7d — filters old buckets
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_with_timeframe() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "timeframe-job").await;

        let today = Utc::now().date_naive();
        let recent = (today - chrono::Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();
        let old = (today - chrono::Duration::days(60))
            .format("%Y-%m-%d")
            .to_string();

        let manifest = make_manifest_with_buckets(
            job.id,
            &[(old.as_str(), 10, 5.00), (recent.as_str(), 2, 1.00)],
        );
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/cost-summary?timeframe=7d", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let summary = &json["summary"];
        // Only the recent bucket (2 runs, $1.00) should be in the 7d window
        assert_eq!(summary["total_runs"], 2);
        assert!((summary["total_cost_usd"].as_f64().unwrap() - 1.00).abs() < 0.001);

        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 1);
    }

    // =======================================================================
    // 24. GET /api/jobs/{id}/cost-summary?start=...&end=... — custom range
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_custom_range() {
        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "custom-range-job").await;

        let manifest = make_manifest_with_buckets(
            job.id,
            &[
                ("2026-01-10", 4, 2.00),
                ("2026-01-20", 3, 1.50),
                ("2026-02-05", 5, 3.00),
            ],
        );
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        // Request only January 2026
        let uri = format!(
            "/api/jobs/{}/cost-summary?start=2026-01-01&end=2026-01-31",
            job.id
        );
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let summary = &json["summary"];
        // Only the two January buckets: 4+3=7 runs, $2.00+$1.50=$3.50
        assert_eq!(summary["total_runs"], 7);
        assert!((summary["total_cost_usd"].as_f64().unwrap() - 3.50).abs() < 0.001);

        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 2);
    }

    // =======================================================================
    // 25. GET /api/costs/summary — two jobs, manifests present
    // =======================================================================
    #[tokio::test]
    async fn test_get_global_cost_summary() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job_a = create_job_in_store(&job_store, "global-job-a").await;
        let job_b = create_job_in_store(&job_store, "global-job-b").await;

        let today = Utc::now().date_naive();
        let recent = (today - chrono::Duration::days(3))
            .format("%Y-%m-%d")
            .to_string();

        log_store.seed_manifest(
            job_a.id,
            make_manifest_with_buckets(job_a.id, &[(recent.as_str(), 4, 2.00)]),
        );
        log_store.seed_manifest(
            job_b.id,
            make_manifest_with_buckets(job_b.id, &[(recent.as_str(), 6, 3.00)]),
        );

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/costs/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert!(json["timeframe"].is_string());

        let top_jobs = json["top_jobs"].as_array().unwrap();
        assert_eq!(top_jobs.len(), 2);

        // Top job by cost should be job_b ($3.00 > $2.00)
        assert_eq!(top_jobs[0]["job_id"], job_b.id.to_string());
        assert_eq!(top_jobs[1]["job_id"], job_a.id.to_string());

        let daily_trend = json["daily_trend"].as_array().unwrap();
        assert_eq!(daily_trend.len(), 1);
        // Merged cost for the single date: $2.00 + $3.00 = $5.00
        assert!((daily_trend[0]["cost_usd"].as_f64().unwrap() - 5.00).abs() < 0.001);
    }

    // =======================================================================
    // 26. GET /api/costs/summary — no jobs → zeroed response
    // =======================================================================
    #[tokio::test]
    async fn test_get_global_cost_summary_no_jobs() {
        let state = make_test_state();
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/costs/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["today_usd"], 0.0);
        assert_eq!(json["week_usd"], 0.0);
        assert_eq!(json["month_usd"], 0.0);

        let top_jobs = json["top_jobs"].as_array().unwrap();
        assert!(top_jobs.is_empty());

        let daily_trend = json["daily_trend"].as_array().unwrap();
        assert!(daily_trend.is_empty());
    }

    // =======================================================================
    // 27. GET /api/jobs/{id}/manifest — manifest present
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_manifest_exists() {
        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "manifest-job").await;

        let manifest = make_manifest_with_buckets(job.id, &[("2026-03-30", 5, 2.50)]);
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/manifest", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(json["job_id"], job.id.to_string());
        assert_eq!(json["total_runs"], 5);
        assert!((json["total_cost_usd"].as_f64().unwrap() - 2.50).abs() < 0.001);
        assert!(json["daily_buckets"]["2026-03-30"].is_object());
    }

    // =======================================================================
    // 28. GET /api/jobs/{id}/manifest — non-existent job → 404
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_manifest_not_found() {
        let state = make_test_state();
        let app = make_test_app(state);

        let unknown = uuid::Uuid::now_v7();
        let uri = format!("/api/jobs/{}/manifest", unknown);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    // =======================================================================
    // 29. GET /api/jobs/{id}/manifest — job exists, no manifest → default
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_manifest_no_manifest() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(crate::models::job::NewJob {
                name: "no-manifest-job-2".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: crate::models::job::ExecutionType::ShellCommand("echo hi".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/manifest", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Should return a default manifest with the correct job_id
        assert_eq!(json["job_id"], job.id.to_string());
        assert_eq!(json["total_runs"], 0);
        assert_eq!(json["total_cost_usd"], 0.0);
        assert!(json["daily_buckets"].as_object().unwrap().is_empty());
    }

    // =======================================================================
    // 30. GET /api/jobs/{id}/cost-summary?timeframe=INVALID → 400
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_invalid_timeframe_returns_400() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(crate::models::job::NewJob {
                name: "invalid-tf-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: crate::models::job::ExecutionType::ShellCommand("echo hi".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/cost-summary?timeframe=2weeks", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // =======================================================================
    // 31. GET /api/jobs/{id}/cost-summary?timeframe=7d&start=... → 400 conflict
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_conflicting_params_returns_400() {
        let state = make_test_state();

        let job = state
            .job_store
            .create_job(crate::models::job::NewJob {
                name: "conflict-params-job".to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: crate::models::job::ExecutionType::ShellCommand("echo hi".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                schedule_mode: None,
                pre_hook: None,
                post_hook: None,
                pre_hook_script_type: None,
                post_hook_script_type: None,
            })
            .await
            .unwrap();

        let app = make_test_app(state);

        let uri = format!(
            "/api/jobs/{}/cost-summary?timeframe=7d&start=2026-01-01",
            job.id
        );
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    // =======================================================================
    // 32. GET /api/jobs/{id}/cost-summary — runs_by_status populated from runs
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_runs_by_status() {
        use crate::models::{JobRun, RunStatus};
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "status-job").await;

        // Seed a manifest so the endpoint returns 200 with data
        let today = Utc::now().date_naive();
        let recent = (today - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();
        let manifest = make_manifest_with_buckets(job.id, &[(recent.as_str(), 3, 1.50)]);
        log_store.seed_manifest(job.id, manifest);

        // Seed two completed and one failed run within the default 30d window
        let now = Utc::now();
        for status in [
            RunStatus::Completed,
            RunStatus::Completed,
            RunStatus::Failed,
        ] {
            let run = JobRun {
                run_id: uuid::Uuid::now_v7(),
                job_id: job.id,
                started_at: now - chrono::Duration::days(1),
                finished_at: Some(now),
                status,
                exit_code: Some(0),
                log_size_bytes: 0,
                error: None,
                trigger_params: None,
                total_cost_usd: None,
                duration_ms: None,
                num_turns: None,
                model: None,
                usage: None,
            };
            log_store.create_run(&run).await.unwrap();
        }

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!("/api/jobs/{}/cost-summary", job.id);
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let runs_by_status = json["summary"]["runs_by_status"].as_object().unwrap();
        assert_eq!(
            runs_by_status.get("Completed").and_then(|v| v.as_u64()),
            Some(2)
        );
        assert_eq!(
            runs_by_status.get("Failed").and_then(|v| v.as_u64()),
            Some(1)
        );
    }

    // =======================================================================
    // 33. GET /api/jobs/{id}/cost-summary — daily data sorted by date asc
    // =======================================================================
    #[tokio::test]
    async fn test_get_job_cost_summary_data_sorted_by_date() {
        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "sorted-date-job").await;

        // Insert buckets in non-chronological order to verify sorting
        let manifest = make_manifest_with_buckets(
            job.id,
            &[
                ("2026-01-20", 2, 1.00),
                ("2026-01-10", 1, 0.50),
                ("2026-01-15", 3, 1.50),
            ],
        );
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let uri = format!(
            "/api/jobs/{}/cost-summary?start=2026-01-01&end=2026-01-31",
            job.id
        );
        let response = app
            .oneshot(Request::builder().uri(&uri).body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let data = json["data"].as_array().unwrap();
        assert_eq!(data.len(), 3);

        // Verify ascending date order
        let dates: Vec<&str> = data.iter().map(|d| d["date"].as_str().unwrap()).collect();
        assert_eq!(dates, vec!["2026-01-10", "2026-01-15", "2026-01-20"]);
    }

    // =======================================================================
    // 34. GET /api/costs/summary — top_jobs capped at 5 even with 6+ jobs
    // =======================================================================
    #[tokio::test]
    async fn test_get_global_cost_summary_top_jobs_capped_at_5() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let today = Utc::now().date_naive();
        let recent = (today - chrono::Duration::days(2))
            .format("%Y-%m-%d")
            .to_string();

        // Create 6 jobs, each with a distinct cost
        for i in 0..6u64 {
            let job = create_job_in_store(&job_store, &format!("cap-job-{}", i)).await;
            let cost = (i + 1) as f64 * 1.0; // costs: 1.0, 2.0, 3.0, 4.0, 5.0, 6.0
            log_store.seed_manifest(
                job.id,
                make_manifest_with_buckets(job.id, &[(recent.as_str(), 1, cost)]),
            );
        }

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/costs/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let top_jobs = json["top_jobs"].as_array().unwrap();
        // Max 5 jobs returned
        assert_eq!(top_jobs.len(), 5);

        // First entry must have the highest cost (6.0)
        let first_cost = top_jobs[0]["total_cost"].as_f64().unwrap();
        assert!(
            (first_cost - 6.0).abs() < 0.001,
            "expected 6.0, got {}",
            first_cost
        );

        // Last entry in top-5 must have cost 2.0 (jobs sorted desc: 6,5,4,3,2)
        let last_cost = top_jobs[4]["total_cost"].as_f64().unwrap();
        assert!(
            (last_cost - 2.0).abs() < 0.001,
            "expected 2.0, got {}",
            last_cost
        );
    }

    // =======================================================================
    // 35. GET /api/costs/summary — today_usd, week_usd, month_usd verified
    // =======================================================================
    #[tokio::test]
    async fn test_get_global_cost_summary_period_totals() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "period-totals-job").await;

        let today = Utc::now().date_naive();
        let today_str = today.format("%Y-%m-%d").to_string();
        let week_ago = (today - chrono::Duration::days(3))
            .format("%Y-%m-%d")
            .to_string();
        let month_ago = (today - chrono::Duration::days(20))
            .format("%Y-%m-%d")
            .to_string();
        // This date is outside the month window (30d) and week window
        let old = (today - chrono::Duration::days(60))
            .format("%Y-%m-%d")
            .to_string();

        let manifest = make_manifest_with_buckets(
            job.id,
            &[
                (today_str.as_str(), 1, 1.00), // in today, week, month
                (week_ago.as_str(), 2, 2.00),  // in week and month, not today
                (month_ago.as_str(), 3, 3.00), // in month only
                (old.as_str(), 4, 4.00),       // outside all windows
            ],
        );
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/costs/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // today_usd should be 1.00 (only today's bucket)
        let today_usd = json["today_usd"].as_f64().unwrap();
        assert!(
            (today_usd - 1.00).abs() < 0.001,
            "today_usd expected 1.00, got {}",
            today_usd
        );

        // week_usd covers last 7 days (days 0-6): today(1.00) + week_ago(2.00) = 3.00
        let week_usd = json["week_usd"].as_f64().unwrap();
        assert!(
            (week_usd - 3.00).abs() < 0.001,
            "week_usd expected 3.00, got {}",
            week_usd
        );

        // month_usd covers last 30 days (days 0-29): today(1.00) + week_ago(2.00) + month_ago(3.00) = 6.00
        let month_usd = json["month_usd"].as_f64().unwrap();
        assert!(
            (month_usd - 6.00).abs() < 0.001,
            "month_usd expected 6.00, got {}",
            month_usd
        );
    }

    // =======================================================================
    // 36. GET /api/costs/summary — today_tokens populated from today's bucket
    // =======================================================================
    #[tokio::test]
    async fn test_get_global_cost_summary_today_tokens() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        let job = create_job_in_store(&job_store, "today-tokens-job").await;

        let today = Utc::now().date_naive();
        let today_str = today.format("%Y-%m-%d").to_string();

        // make_manifest_with_buckets seeds: input_tokens = runs * 1000, output_tokens = runs * 250
        // For 4 runs: input=4000, output=1000
        let manifest = make_manifest_with_buckets(job.id, &[(today_str.as_str(), 4, 2.00)]);
        log_store.seed_manifest(job.id, manifest);

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/costs/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        let today_tokens = &json["today_tokens"];
        let input = today_tokens["input"].as_u64().unwrap();
        let output = today_tokens["output"].as_u64().unwrap();

        // 4 runs * 1000 input = 4000; 4 runs * 250 output = 1000
        assert_eq!(input, 4000, "expected 4000 input tokens, got {}", input);
        assert_eq!(output, 1000, "expected 1000 output tokens, got {}", output);
    }

    // =======================================================================
    // 37. GET /api/costs/summary — mix of jobs with/without manifests
    // =======================================================================
    #[tokio::test]
    async fn test_get_global_cost_summary_mixed_manifests() {
        use chrono::Utc;

        let job_store = Arc::new(InMemoryJobStore::new());
        let log_store = Arc::new(InMemoryLogStore::new());

        // job_with_manifest: has cost data
        let job_with = create_job_in_store(&job_store, "mixed-with-manifest").await;
        // job_without_manifest: no manifest seeded
        let _job_without = create_job_in_store(&job_store, "mixed-no-manifest").await;

        let today = Utc::now().date_naive();
        let recent = (today - chrono::Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        log_store.seed_manifest(
            job_with.id,
            make_manifest_with_buckets(job_with.id, &[(recent.as_str(), 3, 1.50)]),
        );

        let state = make_test_state_with_stores(
            job_store as Arc<dyn JobStore>,
            log_store as Arc<dyn LogStore>,
        );
        let app = make_test_app(state);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/api/costs/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);

        let body = body_string(response.into_body()).await;
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();

        // Only the job with a manifest should appear in top_jobs
        let top_jobs = json["top_jobs"].as_array().unwrap();
        assert_eq!(top_jobs.len(), 1, "only 1 job with manifest should appear");
        assert_eq!(top_jobs[0]["job_id"], job_with.id.to_string());

        // month_usd should reflect only the job with a manifest ($1.50)
        let month_usd = json["month_usd"].as_f64().unwrap();
        assert!(
            (month_usd - 1.50).abs() < 0.001,
            "month_usd expected 1.50, got {}",
            month_usd
        );
    }
}
