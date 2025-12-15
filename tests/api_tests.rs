//! Integration tests for the HTTP API.
//!
//! These tests spawn a real Axum server on a random port and use reqwest
//! to hit it with actual HTTP requests.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agent_cron_scheduler::daemon::events::JobEvent;
use agent_cron_scheduler::models::{DaemonConfig, Job, JobRun, JobUpdate, NewJob};
use agent_cron_scheduler::server::{self, AppState};
use agent_cron_scheduler::storage::{JobStore, LogStore};

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::{broadcast, Notify, RwLock};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// In-memory stores for integration testing
// ---------------------------------------------------------------------------

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
        if let Some(n) = update.name {
            job.name = n;
        }
        if let Some(s) = update.schedule {
            job.schedule = s;
        }
        if let Some(x) = update.execution {
            job.execution = x;
        }
        if let Some(e) = update.enabled {
            job.enabled = e;
        }
        if let Some(t) = update.timezone {
            job.timezone = Some(t);
        }
        if let Some(w) = update.working_dir {
            job.working_dir = Some(w);
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
        let len_before = jobs.len();
        jobs.retain(|j| j.id != id);
        if jobs.len() == len_before {
            return Err(anyhow::anyhow!("not found"));
        }
        Ok(())
    }
}

struct InMemoryLogStore;

#[async_trait]
impl LogStore for InMemoryLogStore {
    async fn create_run(&self, _run: &JobRun) -> anyhow::Result<()> {
        Ok(())
    }
    async fn update_run(&self, _run: &JobRun) -> anyhow::Result<()> {
        Ok(())
    }
    async fn append_log(&self, _job_id: Uuid, _run_id: Uuid, _data: &[u8]) -> anyhow::Result<()> {
        Ok(())
    }
    async fn read_log(
        &self,
        _job_id: Uuid,
        _run_id: Uuid,
        _tail: Option<usize>,
    ) -> anyhow::Result<String> {
        Ok(String::new())
    }
    async fn list_runs(
        &self,
        _job_id: Uuid,
        _limit: usize,
        _offset: usize,
    ) -> anyhow::Result<(Vec<JobRun>, usize)> {
        Ok((vec![], 0))
    }
    async fn cleanup(&self, _job_id: Uuid, _max_files: usize) -> anyhow::Result<()> {
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper to spawn a test server on a random port
// ---------------------------------------------------------------------------

async fn spawn_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
    let state = Arc::new(AppState {
        job_store: Arc::new(InMemoryJobStore::new()),
        log_store: Arc::new(InMemoryLogStore),
        event_tx,
        scheduler_notify: Arc::new(Notify::new()),
        config: Arc::new(DaemonConfig::default()),
        start_time: Instant::now(),
        active_runs: Arc::new(RwLock::new(HashMap::new())),
        shutdown_tx: None,
        dispatch_tx: None,
    });

    let router = server::create_router(state);

    // Bind to port 0 to get a random available port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to random port");
    let addr = listener.local_addr().expect("get local addr");
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, handle)
}

fn new_job_json(name: &str) -> serde_json::Value {
    serde_json::json!({
        "name": name,
        "schedule": "*/5 * * * *",
        "execution": {
            "type": "ShellCommand",
            "value": "echo hello"
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_health_endpoint_returns_correct_structure() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/health", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["uptime_seconds"].is_number());
    assert!(json["active_jobs"].is_number());
    assert!(json["total_jobs"].is_number());
    assert_eq!(json["version"], "0.1.0");
}

#[tokio::test]
async fn test_create_job_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("integration-test-job"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 201);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "integration-test-job");
    assert!(json["id"].is_string());
}

#[tokio::test]
async fn test_list_jobs_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    // Create two jobs
    client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("list-job-1"))
        .send()
        .await
        .unwrap();
    client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("list-job-2"))
        .send()
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/api/jobs", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: Vec<serde_json::Value> = resp.json().await.unwrap();
    assert_eq!(json.len(), 2);
}

#[tokio::test]
async fn test_get_job_by_name_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let create_resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("fetch-by-name"))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let job_id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{}/api/jobs/fetch-by-name", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "fetch-by-name");
    assert_eq!(json["id"], job_id);
}

#[tokio::test]
async fn test_update_job_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let create_resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("update-me"))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let job_id = created["id"].as_str().unwrap();

    let resp = client
        .patch(format!("{}/api/jobs/{}", base_url, job_id))
        .json(&serde_json::json!({"name": "updated-name"}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "updated-name");
}

#[tokio::test]
async fn test_delete_job_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let create_resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("delete-me"))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let job_id = created["id"].as_str().unwrap();

    let resp = client
        .delete(format!("{}/api/jobs/{}", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 204);

    // Verify it's gone
    let get_resp = client
        .get(format!("{}/api/jobs/{}", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(get_resp.status(), 404);
}

#[tokio::test]
async fn test_enable_disable_job_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let create_resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("toggle-me"))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let job_id = created["id"].as_str().unwrap();

    // Disable
    let resp = client
        .post(format!("{}/api/jobs/{}/disable", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["enabled"], false);

    // Enable
    let resp = client
        .post(format!("{}/api/jobs/{}/enable", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["enabled"], true);
}

#[tokio::test]
async fn test_trigger_job_via_http() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let create_resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("trigger-me"))
        .send()
        .await
        .unwrap();
    let created: serde_json::Value = create_resp.json().await.unwrap();
    let job_id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("{}/api/jobs/{}/trigger", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 202);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["message"], "Job triggered");
}

#[tokio::test]
async fn test_error_404_not_found() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/jobs/{}", base_url, Uuid::now_v7()))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["error"].is_string());
    assert!(json["message"].is_string());
}

#[tokio::test]
async fn test_error_409_conflict() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    // Create first job
    client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("conflict-name"))
        .send()
        .await
        .unwrap();

    // Try to create duplicate
    let resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&new_job_json("conflict-name"))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 409);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["error"].is_string());
    assert!(json["message"].is_string());
}

#[tokio::test]
async fn test_error_400_validation() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let bad_job = serde_json::json!({
        "name": "bad-cron",
        "schedule": "not a cron expression",
        "execution": {
            "type": "ShellCommand",
            "value": "echo"
        }
    });

    let resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&bad_job)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 400);

    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["error"].is_string());
    assert!(json["message"].is_string());
}

#[tokio::test]
async fn test_sse_connection() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    // Just verify the SSE endpoint is reachable and starts streaming
    let resp = client
        .get(format!("{}/api/events", base_url))
        .timeout(std::time::Duration::from_millis(500))
        .send()
        .await;

    // The request should either succeed (200) or timeout while streaming
    // Either is acceptable - what matters is no 404/500
    match resp {
        Ok(r) => assert_eq!(r.status(), 200),
        Err(e) => assert!(e.is_timeout(), "Expected timeout, got: {}", e),
    }
}
