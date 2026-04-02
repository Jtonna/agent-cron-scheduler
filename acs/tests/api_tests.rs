//! Integration tests for the HTTP API.
//!
//! These tests spawn a real Axum server on a random port and use reqwest
//! to hit it with actual HTTP requests.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use agent_cron_scheduler::daemon::events::JobEvent;
use agent_cron_scheduler::daemon::executor::Executor;
use agent_cron_scheduler::models::{
    DaemonConfig, DispatchRequest, ExecutionType, Job, JobManifest, JobRun, JobUpdate, NewJob,
};
use agent_cron_scheduler::pty::MockPtySpawner;
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
            log_environment: new.log_environment,
            allow_concurrent: new.allow_concurrent.unwrap_or(false),
            pre_hook: new.pre_hook,
            post_hook: new.post_hook,
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
        if let Some(ac) = update.allow_concurrent {
            job.allow_concurrent = ac;
        }
        if let Some(ph) = update.pre_hook {
            job.pre_hook = Some(ph);
        }
        if let Some(ph) = update.post_hook {
            job.post_hook = Some(ph);
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

struct InMemoryLogStore {
    manifests: RwLock<HashMap<Uuid, JobManifest>>,
}

impl InMemoryLogStore {
    fn new() -> Self {
        Self {
            manifests: RwLock::new(HashMap::new()),
        }
    }

    pub async fn seed_manifest(&self, job_id: Uuid, manifest: JobManifest) {
        self.manifests.write().await.insert(job_id, manifest);
    }
}

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
    async fn read_manifest(&self, job_id: Uuid) -> anyhow::Result<Option<JobManifest>> {
        Ok(self.manifests.read().await.get(&job_id).cloned())
    }
    async fn update_manifest(&self, job_id: Uuid, run: &JobRun) -> anyhow::Result<()> {
        let mut manifests = self.manifests.write().await;
        let manifest = manifests
            .entry(job_id)
            .or_insert_with(|| JobManifest::new(job_id));
        manifest.merge_run(run);
        Ok(())
    }
    async fn rebuild_manifest(&self, _job_id: Uuid) -> anyhow::Result<JobManifest> {
        Ok(JobManifest::new(_job_id))
    }
}

// ---------------------------------------------------------------------------
// Storing in-memory log store for executor-backed tests
// ---------------------------------------------------------------------------

struct StoringLogStore {
    runs: RwLock<Vec<JobRun>>,
    manifests: RwLock<HashMap<Uuid, JobManifest>>,
}

impl StoringLogStore {
    fn new() -> Self {
        Self {
            runs: RwLock::new(Vec::new()),
            manifests: RwLock::new(HashMap::new()),
        }
    }

    pub async fn seed_manifest(&self, job_id: Uuid, manifest: JobManifest) {
        self.manifests.write().await.insert(job_id, manifest);
    }
}

#[async_trait]
impl LogStore for StoringLogStore {
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
    async fn read_manifest(&self, job_id: Uuid) -> anyhow::Result<Option<JobManifest>> {
        Ok(self.manifests.read().await.get(&job_id).cloned())
    }
    async fn update_manifest(&self, job_id: Uuid, run: &JobRun) -> anyhow::Result<()> {
        let mut manifests = self.manifests.write().await;
        let manifest = manifests
            .entry(job_id)
            .or_insert_with(|| JobManifest::new(job_id));
        manifest.merge_run(run);
        Ok(())
    }
    async fn rebuild_manifest(&self, _job_id: Uuid) -> anyhow::Result<JobManifest> {
        Ok(JobManifest::new(_job_id))
    }
}

// ---------------------------------------------------------------------------
// Helper to spawn a test server on a random port
// ---------------------------------------------------------------------------

async fn spawn_test_server() -> (String, tokio::task::JoinHandle<()>) {
    let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
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

/// Spawn a test server wired with a real executor (MockPtySpawner) and a
/// storing log store so that triggered jobs actually run and appear in run
/// history.  Returns (base_url, Arc<StoringLogStore>, server_handle).
async fn spawn_test_server_with_executor(
) -> (String, Arc<StoringLogStore>, tokio::task::JoinHandle<()>) {
    let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
    let log_store: Arc<StoringLogStore> = Arc::new(StoringLogStore::new());
    let active_runs = Arc::new(RwLock::new(HashMap::new()));

    // Create dispatch channel
    let (dispatch_tx, mut dispatch_rx) = tokio::sync::mpsc::channel::<DispatchRequest>(64);

    let state = Arc::new(AppState {
        job_store: Arc::new(InMemoryJobStore::new()),
        log_store: Arc::clone(&log_store) as Arc<dyn LogStore>,
        event_tx: event_tx.clone(),
        scheduler_notify: Arc::new(Notify::new()),
        config: Arc::new(DaemonConfig::default()),
        start_time: Instant::now(),
        active_runs: Arc::clone(&active_runs),
        shutdown_tx: None,
        dispatch_tx: Some(dispatch_tx),
    });

    // Executor using a MockPtySpawner that completes immediately with exit 0
    let pty_spawner: Arc<dyn agent_cron_scheduler::pty::PtySpawner> = Arc::new(
        MockPtySpawner::with_output_and_exit(vec![b"ok\n".to_vec()], 0),
    );
    let executor = Executor::new(
        event_tx.clone(),
        Arc::clone(&log_store) as Arc<dyn LogStore>,
        Arc::new(DaemonConfig::default()),
        pty_spawner,
    );

    // Background dispatch loop
    tokio::spawn(async move {
        while let Some(request) = dispatch_rx.recv().await {
            if let Ok(handle) = executor
                .spawn_job(
                    &request.job,
                    request.run_id,
                    request.trigger_params.as_ref(),
                )
                .await
            {
                let job_id = handle.job_id;
                let mut runs = active_runs.write().await;
                let handles = runs.entry(job_id).or_insert_with(Vec::new);
                handles.retain(|h: &agent_cron_scheduler::daemon::executor::RunHandle| {
                    !h.join_handle.is_finished()
                });
                if request.job.allow_concurrent {
                    handles.push(handle);
                } else {
                    handles.clear();
                    handles.push(handle);
                }
            }
        }
    });

    let router = server::create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to random port");
    let addr = listener.local_addr().expect("get local addr");
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, log_store, handle)
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
    assert_eq!(json["version"], env!("CARGO_PKG_VERSION"));
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

/// Spawn a test server that exposes the InMemoryJobStore and InMemoryLogStore
/// so tests can seed data before making HTTP requests.
/// Returns (base_url, Arc<InMemoryJobStore>, Arc<InMemoryLogStore>, handle).
async fn spawn_test_server_with_stores() -> (
    String,
    Arc<InMemoryJobStore>,
    Arc<InMemoryLogStore>,
    tokio::task::JoinHandle<()>,
) {
    let (event_tx, _) = broadcast::channel::<JobEvent>(4096);
    let job_store = Arc::new(InMemoryJobStore::new());
    let log_store = Arc::new(InMemoryLogStore::new());

    let state = Arc::new(AppState {
        job_store: Arc::clone(&job_store) as Arc<dyn JobStore>,
        log_store: Arc::clone(&log_store) as Arc<dyn LogStore>,
        event_tx,
        scheduler_notify: Arc::new(Notify::new()),
        config: Arc::new(DaemonConfig::default()),
        start_time: Instant::now(),
        active_runs: Arc::new(RwLock::new(HashMap::new())),
        shutdown_tx: None,
        dispatch_tx: None,
    });

    let router = server::create_router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to random port");
    let addr = listener.local_addr().expect("get local addr");
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, job_store, log_store, handle)
}

// ---------------------------------------------------------------------------
// Helpers for building test manifests
// ---------------------------------------------------------------------------

fn make_manifest_with_daily_bucket(
    job_id: Uuid,
    date_key: &str,
    runs: u64,
    cost_usd: f64,
) -> JobManifest {
    use agent_cron_scheduler::models::manifest::TimeBucket;
    use std::collections::BTreeMap;

    let mut daily_buckets = BTreeMap::new();
    daily_buckets.insert(
        date_key.to_string(),
        TimeBucket {
            runs,
            cost_usd,
            duration_ms: 1000 * runs,
            num_turns: runs,
            ..Default::default()
        },
    );

    JobManifest {
        job_id,
        version: 1,
        updated_at: chrono::Utc::now(),
        total_runs: runs,
        total_cost_usd: cost_usd,
        total_duration_ms: 1000 * runs,
        daily_buckets,
        weekly_buckets: BTreeMap::new(),
        monthly_buckets: BTreeMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Integration tests: GET /api/jobs/{id}/cost-summary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_job_cost_summary_happy_path() {
    let (base_url, job_store, log_store, _handle) = spawn_test_server_with_stores().await;
    let client = reqwest::Client::new();

    // Create a job
    let job = job_store
        .create_job(NewJob {
            name: "cost-summary-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo test".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: None,
            pre_hook: None,
            post_hook: None,
        })
        .await
        .unwrap();

    // Seed a manifest with known data using today's date so it falls within any default window
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let manifest = make_manifest_with_daily_bucket(job.id, &today, 3, 0.75);
    log_store.seed_manifest(job.id, manifest).await;

    let resp = client
        .get(format!("{}/api/jobs/{}/cost-summary", base_url, job.id))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(json["job_id"], job.id.to_string());
    assert!(json["timeframe"].is_string());
    assert!(json["summary"].is_object());
    assert_eq!(json["summary"]["total_runs"], 3);
    assert!((json["summary"]["total_cost_usd"].as_f64().unwrap() - 0.75).abs() < 1e-9);
    assert!(json["data"].is_array());
}

#[tokio::test]
async fn test_job_cost_summary_no_manifest_returns_zeroed() {
    let (base_url, job_store, _log_store, _handle) = spawn_test_server_with_stores().await;
    let client = reqwest::Client::new();

    // Create a job but do NOT seed any manifest
    let job = job_store
        .create_job(NewJob {
            name: "no-manifest-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo test".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: None,
            pre_hook: None,
            post_hook: None,
        })
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/api/jobs/{}/cost-summary", base_url, job.id))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(json["job_id"], job.id.to_string());
    assert_eq!(json["summary"]["total_runs"], 0);
    assert_eq!(json["summary"]["total_cost_usd"], 0.0);
    assert_eq!(json["data"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_job_cost_summary_job_not_found_returns_404() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let nonexistent_id = Uuid::now_v7();
    let resp = client
        .get(format!(
            "{}/api/jobs/{}/cost-summary",
            base_url, nonexistent_id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["error"].is_string());
    assert!(json["message"].is_string());
}

#[tokio::test]
async fn test_job_cost_summary_invalid_timeframe_returns_400() {
    let (base_url, job_store, _log_store, _handle) = spawn_test_server_with_stores().await;
    let client = reqwest::Client::new();

    // Create a job so the route can find it
    let job = job_store
        .create_job(NewJob {
            name: "timeframe-test-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo test".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: None,
            pre_hook: None,
            post_hook: None,
        })
        .await
        .unwrap();

    let resp = client
        .get(format!(
            "{}/api/jobs/{}/cost-summary?timeframe=invalid_value",
            base_url, job.id
        ))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 400);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["error"].is_string());
    assert!(json["message"].is_string());
}

// ---------------------------------------------------------------------------
// Integration tests: GET /api/costs/summary
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_global_cost_summary_happy_path_aggregates_multiple_jobs() {
    let (base_url, job_store, log_store, _handle) = spawn_test_server_with_stores().await;
    let client = reqwest::Client::new();

    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // Create two jobs and seed manifests for each
    for (name, cost) in [("global-cost-job-a", 0.50), ("global-cost-job-b", 0.25)] {
        let job = job_store
            .create_job(NewJob {
                name: name.to_string(),
                schedule: "*/5 * * * *".to_string(),
                execution: ExecutionType::ShellCommand("echo test".to_string()),
                enabled: true,
                timezone: None,
                working_dir: None,
                env_vars: None,
                timeout_secs: 0,
                log_environment: false,
                allow_concurrent: None,
                pre_hook: None,
                post_hook: None,
            })
            .await
            .unwrap();
        let manifest = make_manifest_with_daily_bucket(job.id, &today, 2, cost);
        log_store.seed_manifest(job.id, manifest).await;
    }

    let resp = client
        .get(format!("{}/api/costs/summary", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert!(json["timeframe"].is_string());
    assert!(json["today_usd"].is_number());
    // Both jobs contributed today_usd; total must be >= 0.75
    let today_usd = json["today_usd"].as_f64().unwrap();
    assert!(
        today_usd >= 0.74,
        "expected today_usd >= 0.75, got {}",
        today_usd
    );
    assert!(json["week_usd"].is_number());
    assert!(json["month_usd"].is_number());
    assert!(json["today_tokens"].is_object());
    assert!(json["top_jobs"].is_array());
    assert!(json["daily_trend"].is_array());
}

#[tokio::test]
async fn test_global_cost_summary_no_jobs_returns_zeroed() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/costs/summary", base_url))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert!(json["timeframe"].is_string());
    assert_eq!(json["today_usd"], 0.0);
    assert_eq!(json["week_usd"], 0.0);
    assert_eq!(json["month_usd"], 0.0);
    assert_eq!(json["top_jobs"].as_array().unwrap().len(), 0);
    assert_eq!(json["daily_trend"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Integration tests: GET /api/jobs/{id}/manifest
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_get_job_manifest_happy_path() {
    let (base_url, job_store, log_store, _handle) = spawn_test_server_with_stores().await;
    let client = reqwest::Client::new();

    // Create a job
    let job = job_store
        .create_job(NewJob {
            name: "manifest-happy-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo test".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: None,
            pre_hook: None,
            post_hook: None,
        })
        .await
        .unwrap();

    // Seed manifest with known data
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let manifest = make_manifest_with_daily_bucket(job.id, &today, 5, 1.25);
    log_store.seed_manifest(job.id, manifest).await;

    let resp = client
        .get(format!("{}/api/jobs/{}/manifest", base_url, job.id))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(json["job_id"], job.id.to_string());
    assert_eq!(json["total_runs"], 5);
    assert!((json["total_cost_usd"].as_f64().unwrap() - 1.25).abs() < 1e-9);
    assert!(json["daily_buckets"].is_object());
    // Should have our seeded daily bucket
    assert!(json["daily_buckets"][&today].is_object());
    assert_eq!(json["daily_buckets"][&today]["runs"], 5);
}

#[tokio::test]
async fn test_get_job_manifest_no_manifest_returns_default() {
    let (base_url, job_store, _log_store, _handle) = spawn_test_server_with_stores().await;
    let client = reqwest::Client::new();

    // Create a job but do NOT seed any manifest
    let job = job_store
        .create_job(NewJob {
            name: "manifest-empty-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo test".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: None,
            pre_hook: None,
            post_hook: None,
        })
        .await
        .unwrap();

    let resp = client
        .get(format!("{}/api/jobs/{}/manifest", base_url, job.id))
        .send()
        .await
        .unwrap();

    // Per the route implementation, no manifest returns a default manifest (200, not 404)
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(json["job_id"], job.id.to_string());
    assert_eq!(json["total_runs"], 0);
    assert_eq!(json["total_cost_usd"], 0.0);
}

#[tokio::test]
async fn test_get_job_manifest_job_not_found_returns_404() {
    let (base_url, _handle) = spawn_test_server().await;
    let client = reqwest::Client::new();

    let nonexistent_id = Uuid::now_v7();
    let resp = client
        .get(format!("{}/api/jobs/{}/manifest", base_url, nonexistent_id))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), 404);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["error"].is_string());
    assert!(json["message"].is_string());
}

/// Verify that a job created with `allow_concurrent: true` can be triggered
/// multiple times and both runs are recorded in run history.
#[tokio::test]
async fn test_concurrent_job_both_runs_appear_in_history() {
    let (base_url, _log_store, _handle) = spawn_test_server_with_executor().await;
    let client = reqwest::Client::new();

    // Create a job with allow_concurrent: true
    let job_body = serde_json::json!({
        "name": "concurrent-job",
        "schedule": "*/5 * * * *",
        "allow_concurrent": true,
        "execution": {
            "type": "ShellCommand",
            "value": "echo concurrent"
        }
    });

    let create_resp = client
        .post(format!("{}/api/jobs", base_url))
        .json(&job_body)
        .send()
        .await
        .unwrap();
    assert_eq!(create_resp.status(), 201);

    let created: serde_json::Value = create_resp.json().await.unwrap();
    let job_id = created["id"].as_str().unwrap();
    assert_eq!(created["allow_concurrent"], true);

    // Trigger the job twice
    let trigger1 = client
        .post(format!("{}/api/jobs/{}/trigger", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(trigger1.status(), 202);
    let t1_json: serde_json::Value = trigger1.json().await.unwrap();
    let run_id_1 = t1_json["run_id"].as_str().unwrap().to_string();

    let trigger2 = client
        .post(format!("{}/api/jobs/{}/trigger", base_url, job_id))
        .send()
        .await
        .unwrap();
    assert_eq!(trigger2.status(), 202);
    let t2_json: serde_json::Value = trigger2.json().await.unwrap();
    let run_id_2 = t2_json["run_id"].as_str().unwrap().to_string();

    // The two trigger calls must have produced distinct run IDs
    assert_ne!(
        run_id_1, run_id_2,
        "each trigger should produce a unique run_id"
    );

    // Poll run history until both runs are present (executor is async)
    let mut found_runs: Vec<serde_json::Value> = vec![];
    for _ in 0..40 {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let runs_resp = client
            .get(format!("{}/api/jobs/{}/runs?limit=10", base_url, job_id))
            .send()
            .await
            .unwrap();
        assert_eq!(runs_resp.status(), 200);
        let runs_json: serde_json::Value = runs_resp.json().await.unwrap();
        let runs = runs_json["runs"].as_array().unwrap().clone();
        if runs.len() >= 2 {
            found_runs = runs;
            break;
        }
    }

    assert_eq!(
        found_runs.len(),
        2,
        "expected 2 runs in history, got {}",
        found_runs.len()
    );

    // Neither run should have a Failed status
    for run in &found_runs {
        let status = run["status"].as_str().unwrap_or("");
        assert_ne!(
            status,
            "Failed",
            "run {} should not be Failed",
            run["run_id"].as_str().unwrap_or("")
        );
    }

    // Verify both expected run IDs appear in history
    let history_ids: Vec<&str> = found_runs
        .iter()
        .map(|r| r["run_id"].as_str().unwrap_or(""))
        .collect();
    assert!(
        history_ids.contains(&run_id_1.as_str()),
        "run_id_1 ({}) not found in history",
        run_id_1
    );
    assert!(
        history_ids.contains(&run_id_2.as_str()),
        "run_id_2 ({}) not found in history",
        run_id_2
    );
}
