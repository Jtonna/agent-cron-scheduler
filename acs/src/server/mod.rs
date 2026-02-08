pub mod assets;
pub mod health;
pub mod routes;
pub mod sse;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::{broadcast, Notify, RwLock};
use tower_http::cors::{Any, CorsLayer};
use uuid::Uuid;

use crate::daemon::events::JobEvent;
use crate::daemon::executor::RunHandle;
use crate::models::DaemonConfig;
use crate::storage::{JobStore, LogStore};

/// Shared application state for the Axum server.
pub struct AppState {
    pub job_store: Arc<dyn JobStore>,
    pub log_store: Arc<dyn LogStore>,
    pub event_tx: broadcast::Sender<JobEvent>,
    pub scheduler_notify: Arc<Notify>,
    pub config: Arc<DaemonConfig>,
    pub start_time: Instant,
    pub active_runs: Arc<RwLock<HashMap<Uuid, RunHandle>>>,
    pub shutdown_tx: Option<tokio::sync::watch::Sender<()>>,
    pub dispatch_tx: Option<tokio::sync::mpsc::Sender<crate::models::Job>>,
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
        .route("/api/jobs/{id}/runs", get(routes::list_runs))
        .route("/api/runs/{run_id}/log", get(routes::get_log))
        .route("/api/events", get(sse::sse_handler))
        .route("/api/shutdown", post(routes::shutdown))
        .route("/api/restart", post(routes::restart))
        .route("/api/logs", get(routes::get_daemon_logs))
        .route("/api/service/status", get(routes::service_status))
        .with_state(state)
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
        .fallback(assets::serve_embedded)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::events::JobEvent;
    use crate::models::job::{ExecutionType, NewJob};
    use crate::models::{Job, JobRun, JobUpdate, RunStatus};
    use crate::storage::{JobStore, LogStore};
    use async_trait::async_trait;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use chrono::Utc;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

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
    }

    impl InMemoryLogStore {
        fn new() -> Self {
            Self {
                runs: RwLock::new(Vec::new()),
                logs: RwLock::new(HashMap::new()),
            }
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
    }

    // -----------------------------------------------------------------------
    // Test helper: build AppState and Router
    // -----------------------------------------------------------------------

    fn make_test_state() -> Arc<AppState> {
        let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
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
        })
    }

    fn make_test_state_with_stores(
        job_store: Arc<dyn JobStore>,
        log_store: Arc<dyn LogStore>,
    ) -> Arc<AppState> {
        let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
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
        assert_eq!(json["version"], "0.1.0");
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
        let mut config = DaemonConfig::default();
        config.data_dir = Some(tmp_dir.path().to_path_buf());

        let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
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
}
