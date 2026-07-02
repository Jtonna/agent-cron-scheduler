//! Integration tests for the /api/workflows and /api/runs endpoints.
//!
//! Each test spawns a real Axum server on a random port and exercises the
//! workflow API end-to-end.

use std::sync::Arc;
use std::time::Instant;

use agent_cron_scheduler::daemon::cost_cache::CostCache;
use agent_cron_scheduler::daemon::events::WorkflowEvent;
use agent_cron_scheduler::models::workflow::{
    CaptureSpec, FailurePolicy, NewWorkflow, RunStatus, ShellStep, StepDef, StepDefCommon,
    WorkflowRun, WorkflowUpdate,
};
use agent_cron_scheduler::models::DaemonConfig;
use agent_cron_scheduler::server::{self, AppState};
use agent_cron_scheduler::storage::workflow_runs::WorkflowRunStore;
use agent_cron_scheduler::storage::workflows::WorkflowStore;

use chrono::{Duration, Utc};
use tokio::sync::{broadcast, Notify};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Store helpers — use the real Sqlite* implementations on a temp dir
// ---------------------------------------------------------------------------

async fn make_stores() -> (
    Arc<dyn WorkflowStore>,
    Arc<dyn WorkflowRunStore>,
    tempfile::TempDir,
) {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let db_path = tmp.path().join("acs.db");
    let db =
        agent_cron_scheduler::storage::sqlite::init_db(&db_path).expect("init SQLite database");
    let wf_store = agent_cron_scheduler::storage::sqlite::SqliteWorkflowStore::new(&db);
    let run_store = agent_cron_scheduler::storage::sqlite::SqliteWorkflowRunStore::new(&db);
    (
        Arc::new(wf_store) as Arc<dyn WorkflowStore>,
        Arc::new(run_store) as Arc<dyn WorkflowRunStore>,
        tmp,
    )
}

// ---------------------------------------------------------------------------
// Test server helper
// ---------------------------------------------------------------------------

async fn spawn_test_server(
    workflow_store: Arc<dyn WorkflowStore>,
    workflow_run_store: Arc<dyn WorkflowRunStore>,
    data_dir: std::path::PathBuf,
) -> (String, Arc<AppState>, tokio::task::JoinHandle<()>) {
    let (workflow_event_tx, _) = broadcast::channel::<WorkflowEvent>(4096);

    // Route auto-created workflow working_dirs to a subfolder of the test's
    // data_dir (a tempdir) so the test doesn't pollute the user's real
    // Documents folder. The TempDir's `Drop` cleans this up when the test
    // returns.
    let config = DaemonConfig {
        display_workflow_dir_root: Some(data_dir.join("workflow_dirs")),
        data_dir: Some(data_dir),
        ..DaemonConfig::default()
    };

    let display_tz: chrono_tz::Tz = config.display_timezone.parse().unwrap_or(chrono_tz::UTC);
    let cost_cache = Arc::new(CostCache::new(Arc::clone(&workflow_run_store), display_tz));

    let state = Arc::new(AppState {
        scheduler_notify: Arc::new(Notify::new()),
        config: Arc::new(config),
        start_time: Instant::now(),
        shutdown_tx: None,
        workflow_event_tx,
        workflow_store,
        workflow_run_store,
        kill_signals: std::sync::Arc::new(tokio::sync::RwLock::new(
            std::collections::HashMap::new(),
        )),
        cost_cache,
    });

    let router = server::create_router(Arc::clone(&state));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind to random port");
    let addr = listener.local_addr().expect("get local addr");
    let base_url = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        axum::serve(listener, router).await.ok();
    });

    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    (base_url, state, handle)
}

// ---------------------------------------------------------------------------
// Workflow builder helper
// ---------------------------------------------------------------------------

fn shell_step(id: &str, cmd: &str) -> StepDef {
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
        command: cmd.to_string(),
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
        steps: vec![shell_step("step-1", "echo hello")],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// 1. POST creates a workflow with steps (1-step ShellStep echo)
#[tokio::test]
async fn test_create_workflow_with_shell_step() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let body = serde_json::to_string(&make_new_workflow("echo-wf")).unwrap();
    let resp = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST /api/workflows");

    assert_eq!(resp.status(), 201);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "echo-wf");
    assert_eq!(json["version"], 1);
    assert_eq!(json["steps"].as_array().unwrap().len(), 1);
}

/// 2. GET /api/workflows lists includes the created workflow (plain array, no wrapping)
#[tokio::test]
async fn test_list_workflows_includes_created() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let body = serde_json::to_string(&make_new_workflow("list-test")).unwrap();
    client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST");

    let resp = client
        .get(format!("{}/api/workflows", base_url))
        .send()
        .await
        .expect("GET /api/workflows");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    // v4.2.10: response is a plain array again (cost data moved to /api/cost/workflows).
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["name"], "list-test");
}

/// 3. GET /api/workflows/{id} returns the workflow
#[tokio::test]
async fn test_get_workflow_by_uuid() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let body = serde_json::to_string(&make_new_workflow("get-by-uuid")).unwrap();
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{}/api/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET by id");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["id"], id);
    assert_eq!(json["name"], "get-by-uuid");
}

/// 4. GET by name resolves correctly
#[tokio::test]
async fn test_get_workflow_by_name() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let body = serde_json::to_string(&make_new_workflow("get-by-name")).unwrap();
    client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST");

    let resp = client
        .get(format!("{}/api/workflows/get-by-name", base_url))
        .send()
        .await
        .expect("GET by name");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "get-by-name");
}

/// 5. PATCH updates a field, version bumps if a definition field changed
#[tokio::test]
async fn test_patch_workflow_bumps_version() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let body = serde_json::to_string(&make_new_workflow("patch-me")).unwrap();
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["version"], 1);

    let update = WorkflowUpdate {
        name: Some("patch-me-renamed".to_string()),
        ..Default::default()
    };
    let patch_body = serde_json::to_string(&update).unwrap();
    let resp = client
        .patch(format!("{}/api/workflows/{}", base_url, id))
        .header("Content-Type", "application/json")
        .body(patch_body)
        .send()
        .await
        .expect("PATCH");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["name"], "patch-me-renamed");
    // name change is a definition change → version should bump
    assert_eq!(json["version"], 2);
}

/// 6. PATCH with duplicate name returns 409
#[tokio::test]
async fn test_patch_workflow_duplicate_name_returns_409() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    // Create wf-a and wf-b
    for name in ["conflict-a", "conflict-b"] {
        client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow(name)).unwrap())
            .send()
            .await
            .expect("POST");
    }

    // Get wf-b's id
    let wf_b: serde_json::Value = client
        .get(format!("{}/api/workflows/conflict-b", base_url))
        .send()
        .await
        .expect("GET")
        .json()
        .await
        .unwrap();
    let id_b = wf_b["id"].as_str().unwrap();

    // Try to rename wf-b to wf-a
    let update = WorkflowUpdate {
        name: Some("conflict-a".to_string()),
        ..Default::default()
    };
    let resp = client
        .patch(format!("{}/api/workflows/{}", base_url, id_b))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&update).unwrap())
        .send()
        .await
        .expect("PATCH");
    assert_eq!(resp.status(), 409);
}

/// 7. DELETE returns 204 and subsequent GET returns 404
#[tokio::test]
async fn test_delete_workflow() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("to-delete")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let del_resp = client
        .delete(format!("{}/api/workflows/{}", base_url, id))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(del_resp.status(), 204);

    let get_resp = client
        .get(format!("{}/api/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET after delete");
    assert_eq!(get_resp.status(), 404);
}

/// 7b. POST/DELETE /api/workflows/{id}/favorite toggles is_favorited
///     end-to-end via the dedicated endpoints, does not bump version, and
///     returns 404 for unknown workflows.
#[tokio::test]
async fn test_favorite_endpoints_roundtrip() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    // Create a fresh workflow.
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("fav-roundtrip")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["is_favorited"], false);
    let initial_version = created["version"].as_u64().unwrap();

    // POST favorite → 200 with is_favorited=true, version unchanged.
    let fav_resp = client
        .post(format!("{}/api/workflows/{}/favorite", base_url, id))
        .send()
        .await
        .expect("POST favorite");
    assert_eq!(fav_resp.status(), 200);
    let fav_body: serde_json::Value = fav_resp.json().await.unwrap();
    assert_eq!(fav_body["is_favorited"], true);
    assert_eq!(fav_body["version"].as_u64().unwrap(), initial_version);

    // GET reflects the new state.
    let get_resp: serde_json::Value = client
        .get(format!("{}/api/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET")
        .json()
        .await
        .unwrap();
    assert_eq!(get_resp["is_favorited"], true);

    // DELETE favorite → 200 with is_favorited=false.
    let unfav_resp = client
        .delete(format!("{}/api/workflows/{}/favorite", base_url, id))
        .send()
        .await
        .expect("DELETE favorite");
    assert_eq!(unfav_resp.status(), 200);
    let unfav_body: serde_json::Value = unfav_resp.json().await.unwrap();
    assert_eq!(unfav_body["is_favorited"], false);
    assert_eq!(unfav_body["version"].as_u64().unwrap(), initial_version);
}

#[tokio::test]
async fn test_favorite_endpoint_unknown_workflow_returns_404() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let bogus = uuid::Uuid::now_v7();
    let resp = client
        .post(format!("{}/api/workflows/{}/favorite", base_url, bogus))
        .send()
        .await
        .expect("POST favorite");
    assert_eq!(resp.status(), 404);
}

/// 7c. PATCH /api/workflows/{id} with is_favorited toggles the flag and
///     leaves version untouched.
#[tokio::test]
async fn test_patch_is_favorited_does_not_bump_version() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("fav-patch-int")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();
    let initial_version = created["version"].as_u64().unwrap();

    let resp = client
        .patch(format!("{}/api/workflows/{}", base_url, id))
        .header("Content-Type", "application/json")
        .body(r#"{"is_favorited": true}"#)
        .send()
        .await
        .expect("PATCH");
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["is_favorited"], true);
    assert_eq!(body["version"].as_u64().unwrap(), initial_version);
}

/// 8. POST /api/workflows/{id}/trigger returns 202 with run_id
#[tokio::test]
async fn test_trigger_workflow_returns_202() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("trigger-test")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let trigger_resp = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("POST trigger");
    assert_eq!(trigger_resp.status(), 202);
    let trigger_json: serde_json::Value = trigger_resp.json().await.unwrap();
    assert!(
        trigger_json["run_id"].is_string(),
        "run_id should be present"
    );
    assert_eq!(trigger_json["workflow_id"], id);
    assert_eq!(trigger_json["workflow_version"], 1);
}

/// 9. After trigger, polling GET /api/runs/{run_id} eventually shows status=Completed
#[tokio::test]
async fn test_trigger_and_poll_until_completed() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let echo_cmd = "echo hello";

    let wf = NewWorkflow {
        name: "poll-test".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![shell_step("step-1", echo_cmd)],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap();

    // Poll up to 10 seconds
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("Run {} did not complete within 10s", run_id);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let run_resp = client
            .get(format!("{}/api/runs/{}", base_url, run_id))
            .send()
            .await
            .expect("GET /api/runs/{run_id}");
        if run_resp.status() == 200 {
            let run_json: serde_json::Value = run_resp.json().await.unwrap();
            let status = run_json["status"].as_str().unwrap_or("");
            match status {
                "Completed" | "Failed" | "Killed" => {
                    assert_eq!(
                        status, "Completed",
                        "expected Completed, got {} for run {}",
                        status, run_id
                    );
                    break;
                }
                _ => {
                    // Still running — keep polling
                }
            }
        }
    }
}

/// 10. SSE: subscribe to /api/events/workflows, trigger a workflow,
///     observe at minimum a RunStarted event for that run_id.
#[tokio::test]
async fn test_sse_workflow_events_run_started() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create workflow
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("sse-test")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    // Subscribe to workflow_event_tx before triggering
    let mut rx = state.workflow_event_tx.subscribe();

    // Trigger the workflow
    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id_str = trigger_json["run_id"].as_str().unwrap();
    let run_id = Uuid::parse_str(run_id_str).unwrap();

    // Wait up to 5 seconds for RunStarted event
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(5);
    let mut got_run_started = false;
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(tokio::time::Duration::from_millis(500), rx.recv()).await {
            Ok(Ok(WorkflowEvent::RunStarted { run_id: r, .. })) if r == run_id => {
                got_run_started = true;
                break;
            }
            Ok(Ok(_)) => {
                // Other event — keep looking
            }
            Ok(Err(_)) | Err(_) => {
                // Channel closed or timeout — keep trying
                break;
            }
        }
    }

    assert!(
        got_run_started,
        "Expected RunStarted event for run {} within 5s",
        run_id
    );
}

/// 11. Trigger with input that's referenced by step's command template:
///     input flows through and the step runs successfully.
#[tokio::test]
async fn test_trigger_with_input_flows_through() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let wf = NewWorkflow {
        name: "input-flow-test".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![shell_step("step-1", "echo input-test-done")],
        default_input: Some(serde_json::json!({"greeting": "hello"})),
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    // Trigger with overridden input
    let trigger_body = r#"{"input": {"greeting": "world"}}"#;
    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, id))
        .header("Content-Type", "application/json")
        .body(trigger_body)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap();

    // Poll until completed
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("Run {} did not complete within 10s", run_id);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let run_resp = client
            .get(format!("{}/api/runs/{}", base_url, run_id))
            .send()
            .await
            .expect("GET run");
        if run_resp.status() == 200 {
            let run_json: serde_json::Value = run_resp.json().await.unwrap();
            let status = run_json["status"].as_str().unwrap_or("");
            match status {
                "Completed" => {
                    // Verify the trigger_input was stored in the run
                    assert_eq!(
                        run_json["trigger_input"]["greeting"], "world",
                        "trigger_input should reflect the provided input"
                    );
                    break;
                }
                "Failed" | "Killed" => {
                    panic!(
                        "Run {} ended with unexpected status {}: {:?}",
                        run_id, status, run_json
                    );
                }
                _ => {
                    // Still running
                }
            }
        }
    }
}

/// 12. Triggering a workflow persists the run record (read it back through the
///     run store).
#[tokio::test]
async fn test_trigger_persists_run_to_disk() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let tmp_path = tmp.path().to_path_buf();
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp_path.clone(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("persist-run-test")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id_str = trigger_json["run_id"].as_str().unwrap();
    let run_id = uuid::Uuid::parse_str(run_id_str).expect("parse run_id");

    // Wait briefly so the create_run call has committed, then verify the
    // record is readable through the store (which is backed by acs.db).
    tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

    let stored = run_store
        .get_run(run_id)
        .await
        .expect("get_run")
        .expect("run record should be persisted");
    assert_eq!(stored.run_id, run_id);
    assert_eq!(stored.workflow_id.to_string(), wf_id);

    // The DB file lives at <data_dir>/acs.db; sanity-check it exists too.
    assert!(
        tmp_path.join("acs.db").exists(),
        "acs.db should exist under the data dir"
    );
}

// ---------------------------------------------------------------------------
// List workflow runs tests (ACS-18 follow-up)
// ---------------------------------------------------------------------------

/// Helper: insert N run records directly into the run store for a workflow.
async fn insert_runs(
    run_store: &Arc<dyn WorkflowRunStore>,
    workflow_id: uuid::Uuid,
    wf: &agent_cron_scheduler::models::workflow::Workflow,
    count: usize,
) -> Vec<uuid::Uuid> {
    let mut run_ids = vec![];
    for _ in 0..count {
        // Small delay so UUIDv7 ordering is distinct.
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
        let run_id = uuid::Uuid::now_v7();
        let run = WorkflowRun {
            run_id,
            workflow_id,
            workflow_version: wf.version,
            workflow_snapshot: wf.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Completed,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        };
        run_store.create_run(run).await.expect("insert run");
        run_ids.push(run_id);
    }
    run_ids
}

/// 14. GET /api/workflows/{id}/runs returns runs for the workflow (total=3).
#[tokio::test]
async fn test_list_runs_endpoint_returns_runs_for_workflow() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create a workflow.
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("runs-test")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();

    // Fetch the stored workflow for the snapshot.
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Insert 3 runs directly.
    insert_runs(&run_store, wf_id, &wf, 3).await;

    let resp = client
        .get(format!("{}/api/workflows/{}/runs", base_url, wf_id_str))
        .send()
        .await
        .expect("GET runs");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["total"], 3);
    assert_eq!(json["runs"].as_array().unwrap().len(), 3);
}

/// 15. GET /api/workflows/{id}/runs?limit=2&offset=1 returns exactly 2 runs.
#[tokio::test]
async fn test_list_runs_endpoint_pagination() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("pagination-test")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();

    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
    insert_runs(&run_store, wf_id, &wf, 5).await;

    let resp = client
        .get(format!(
            "{}/api/workflows/{}/runs?limit=2&offset=1",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET runs paginated");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    // total still reflects all 5 runs
    assert_eq!(json["total"], 5);
    // but only 2 returned for this page
    assert_eq!(json["runs"].as_array().unwrap().len(), 2);
}

/// Kill-endpoint test: trigger a long-running workflow, POST /kill, verify Killed status.
#[tokio::test]
async fn test_kill_endpoint_terminates_running_step() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Platform-appropriate long-sleep command.
    #[cfg(windows)]
    let sleep_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
    #[cfg(not(windows))]
    let sleep_cmd = "sleep 30";

    let wf = NewWorkflow {
        name: "kill-me".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![shell_step("slow", sleep_cmd)],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap();

    // Give the executor a moment to start.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Send the kill.
    let kill_resp = client
        .post(format!("{}/api/runs/{}/kill", base_url, run_id))
        .send()
        .await
        .expect("POST /kill");
    assert_eq!(kill_resp.status(), 202, "kill endpoint should return 202");

    // Poll until the run finishes (should be well under 5s after kill).
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(8);
    let mut final_status = String::new();
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Run {} did not finish within 8s after kill (status: {})",
                run_id, final_status
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let run_resp = client
            .get(format!("{}/api/runs/{}", base_url, run_id))
            .send()
            .await
            .expect("GET run");
        if run_resp.status() == 200 {
            let run_json: serde_json::Value = run_resp.json().await.unwrap();
            let status = run_json["status"].as_str().unwrap_or("").to_string();
            match status.as_str() {
                "Killed" | "Failed" | "Completed" => {
                    final_status = status;
                    break;
                }
                _ => {}
            }
        }
    }

    assert_eq!(
        final_status, "Killed",
        "run should finish with Killed status after kill signal"
    );
}

/// Kill-endpoint 404 test: POST /kill for a non-existent run_id returns 404.
#[tokio::test]
async fn test_kill_endpoint_404_for_unknown_run() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let unknown_run_id = Uuid::now_v7();
    let resp = client
        .post(format!("{}/api/runs/{}/kill", base_url, unknown_run_id))
        .send()
        .await
        .expect("POST /kill for unknown run");

    assert_eq!(resp.status(), 404, "expected 404 for unknown run_id");
}

/// 16. GET /api/workflows/{id}/runs returns runs in latest-first order.
#[tokio::test]
async fn test_list_runs_endpoint_returns_latest_first() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("order-test")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();

    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
    // insert_runs returns run_ids in creation order (oldest first).
    let run_ids = insert_runs(&run_store, wf_id, &wf, 3).await;

    let resp = client
        .get(format!("{}/api/workflows/{}/runs", base_url, wf_id_str))
        .send()
        .await
        .expect("GET runs")
        .json::<serde_json::Value>()
        .await
        .unwrap();

    let runs = resp["runs"].as_array().unwrap();
    // The API returns latest-first: runs[0] should be run_ids[2] (newest).
    assert_eq!(
        runs[0]["run_id"].as_str().unwrap(),
        run_ids[2].to_string(),
        "first result should be the newest run"
    );
    assert_eq!(
        runs[2]["run_id"].as_str().unwrap(),
        run_ids[0].to_string(),
        "last result should be the oldest run"
    );
}

/// 17. GET /api/workflows/nonexistent/runs returns 404.
#[tokio::test]
async fn test_list_runs_endpoint_404_for_unknown_workflow() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/workflows/nonexistent/runs", base_url))
        .send()
        .await
        .expect("GET runs for unknown workflow");
    assert_eq!(resp.status(), 404);
}

/// 18. GET /api/workflows/{id}/runs for a workflow with no runs returns empty array.
#[tokio::test]
async fn test_list_runs_endpoint_zero_runs_returns_empty_array() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("no-runs-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{}/api/workflows/{}/runs", base_url, wf_id_str))
        .send()
        .await
        .expect("GET runs")
        .json::<serde_json::Value>()
        .await
        .unwrap();

    assert_eq!(resp["total"], 0);
    assert!(resp["runs"].as_array().unwrap().is_empty());
}

/// 19. GET /api/workflows/{name}/runs resolves the workflow by name, not UUID.
#[tokio::test]
async fn test_list_runs_resolves_workflow_by_name() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("named-runs-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();

    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
    insert_runs(&run_store, wf_id, &wf, 2).await;

    // Use the workflow name instead of its UUID.
    let resp = client
        .get(format!("{}/api/workflows/named-runs-wf/runs", base_url))
        .send()
        .await
        .expect("GET runs by name")
        .json::<serde_json::Value>()
        .await
        .unwrap();

    assert_eq!(resp["total"], 2);
    assert_eq!(resp["runs"].as_array().unwrap().len(), 2);
}

/// 20. GET /api/runs/recent returns a chronologically-merged feed across
///     workflows, latest-first, with `total` reflecting the global count.
#[tokio::test]
async fn test_list_recent_runs_merges_across_workflows() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Two workflows, two runs each (4 total).
    for name in ["recent-wf-a", "recent-wf-b"] {
        let created: serde_json::Value = client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow(name)).unwrap())
            .send()
            .await
            .expect("POST workflow")
            .json()
            .await
            .unwrap();
        let wf_id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
        let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
        insert_runs(&run_store, wf_id, &wf, 2).await;
    }

    let resp = client
        .get(format!("{}/api/runs/recent", base_url))
        .send()
        .await
        .expect("GET /api/runs/recent");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(json["total"], 4);
    let runs = json["runs"].as_array().unwrap();
    assert_eq!(runs.len(), 4);

    // Latest-first: run_ids are Uuid v7, so descending lexicographic order.
    let ids: Vec<&str> = runs.iter().map(|r| r["run_id"].as_str().unwrap()).collect();
    let mut sorted = ids.clone();
    sorted.sort_by(|a, b| b.cmp(a));
    assert_eq!(ids, sorted, "runs must be ordered latest-first");

    // Mixed workflows: more than one distinct workflow_id should appear.
    let mut wf_ids: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for r in runs {
        wf_ids.insert(r["workflow_id"].as_str().unwrap());
    }
    assert_eq!(
        wf_ids.len(),
        2,
        "feed should include runs from both workflows"
    );

    // Each run carries the embedded workflow_snapshot with the workflow name.
    for r in runs {
        assert!(r["workflow_snapshot"]["name"].is_string());
    }
}

/// 21. GET /api/runs/recent?limit=2&offset=1 returns exactly 2 runs;
///     `total` still reflects the full global count.
#[tokio::test]
async fn test_list_recent_runs_pagination() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("recent-paging")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
    insert_runs(&run_store, wf_id, &wf, 5).await;

    let resp = client
        .get(format!("{}/api/runs/recent?limit=2&offset=1", base_url))
        .send()
        .await
        .expect("GET recent paginated");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["total"], 5);
    assert_eq!(json["runs"].as_array().unwrap().len(), 2);
}

/// 22. GET /api/runs/recent on an empty store returns total=0 and an empty
///     runs array, and is matched as a static route (not interpreted as a
///     run_id by the /api/runs/{run_id} handler).
#[tokio::test]
async fn test_list_recent_runs_empty_store() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{}/api/runs/recent", base_url))
        .send()
        .await
        .expect("GET /api/runs/recent empty");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["total"], 0);
    assert_eq!(json["runs"].as_array().unwrap().len(), 0);
}

// ---------------------------------------------------------------------------
// Cost summary helpers
// ---------------------------------------------------------------------------

/// Insert a single terminal run directly into the store with explicit
/// `finished_at` and `total_cost_usd`.
async fn insert_terminal_run(
    run_store: &Arc<dyn WorkflowRunStore>,
    workflow_id: uuid::Uuid,
    wf: &agent_cron_scheduler::models::workflow::Workflow,
    status: RunStatus,
    finished_at: chrono::DateTime<Utc>,
    total_cost_usd: Option<f64>,
) -> uuid::Uuid {
    insert_terminal_run_with_tokens(
        run_store,
        workflow_id,
        wf,
        status,
        finished_at,
        total_cost_usd,
        0,
        0,
    )
    .await
}

/// Like `insert_terminal_run`, but also sets `total_input_tokens` and
/// `total_output_tokens` on the WorkflowRun. Used by v4.2.11 token tests.
#[allow(clippy::too_many_arguments)]
async fn insert_terminal_run_with_tokens(
    run_store: &Arc<dyn WorkflowRunStore>,
    workflow_id: uuid::Uuid,
    wf: &agent_cron_scheduler::models::workflow::Workflow,
    status: RunStatus,
    finished_at: chrono::DateTime<Utc>,
    total_cost_usd: Option<f64>,
    total_input_tokens: u64,
    total_output_tokens: u64,
) -> uuid::Uuid {
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let run_id = uuid::Uuid::now_v7();
    let run = WorkflowRun {
        run_id,
        workflow_id,
        workflow_version: wf.version,
        workflow_snapshot: wf.clone(),
        started_at: finished_at - Duration::seconds(10),
        finished_at: Some(finished_at),
        status,
        trigger_input: None,
        steps: vec![],
        total_cost_usd,
        total_duration_ms: Some(10_000),
        total_input_tokens,
        total_output_tokens,
    };
    run_store
        .create_run(run)
        .await
        .expect("insert terminal run");
    run_id
}

/// Insert a single running (non-terminal) run with no `finished_at`.
async fn insert_running_run(
    run_store: &Arc<dyn WorkflowRunStore>,
    workflow_id: uuid::Uuid,
    wf: &agent_cron_scheduler::models::workflow::Workflow,
) -> uuid::Uuid {
    tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    let run_id = uuid::Uuid::now_v7();
    let run = WorkflowRun {
        run_id,
        workflow_id,
        workflow_version: wf.version,
        workflow_snapshot: wf.clone(),
        started_at: Utc::now(),
        finished_at: None,
        status: RunStatus::Running,
        trigger_input: None,
        steps: vec![],
        total_cost_usd: None,
        total_duration_ms: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
    };
    run_store.create_run(run).await.expect("insert running run");
    run_id
}

// ---------------------------------------------------------------------------
// Cost summary tests (v4.2.8)
// ---------------------------------------------------------------------------

/// CS-1. cost_summary is present on every workflow entry in GET /api/cost/workflows.
/// (v4.2.10: cost data relocated from /api/workflows to /api/cost/workflows)
#[tokio::test]
async fn test_cost_summary_present_on_get_cost_workflows_list() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create 2 workflows with no runs.
    for name in ["cs-list-wf-a", "cs-list-wf-b"] {
        client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow(name)).unwrap())
            .send()
            .await
            .expect("POST workflow")
            .error_for_status()
            .expect("201 created");
    }

    let resp = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET /api/cost/workflows");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();
    // v4.2.10: /api/cost/workflows returns { workflows: Vec<WorkflowCostEntry>, system_cost_summary }.
    let arr = json["workflows"]
        .as_array()
        .expect("workflows field is array");
    assert_eq!(arr.len(), 2);

    for entry in arr {
        let cs = &entry["cost_summary"];
        assert!(!cs.is_null(), "cost_summary must be present");
        assert_eq!(cs["last_30_days_total_usd"], 0.0);
        assert_eq!(cs["last_30_days_runs"], 0);
        assert_eq!(cs["last_year_total_usd"], 0.0);
        assert_eq!(cs["last_year_runs"], 0);
        assert!(
            cs["computed_at"].is_string(),
            "computed_at must be an ISO-8601 string"
        );
        // Verify it actually parses as a timestamp.
        chrono::DateTime::parse_from_rfc3339(cs["computed_at"].as_str().unwrap())
            .expect("computed_at should parse as RFC-3339");
    }
}

/// CS-2. cost_summary is present on GET /api/cost/workflows/{id}.
/// (v4.2.10: cost data relocated from /api/workflows/{id} to /api/cost/workflows/{id})
#[tokio::test]
async fn test_cost_summary_present_on_get_cost_workflow_singular() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-singular-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!("{}/api/cost/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    // v4.2.10: /api/cost/workflows/{id} returns { workflow_id, workflow_name, cost_summary }.
    let cs = &json["cost_summary"];
    assert!(!cs.is_null(), "cost_summary must be present");
    assert_eq!(cs["last_30_days_total_usd"], 0.0);
    assert_eq!(cs["last_30_days_runs"], 0);
    assert_eq!(cs["last_year_total_usd"], 0.0);
    assert_eq!(cs["last_year_runs"], 0);
    assert!(cs["computed_at"].is_string());
    chrono::DateTime::parse_from_rfc3339(cs["computed_at"].as_str().unwrap())
        .expect("computed_at should parse as RFC-3339");
}

/// CS-3. cost_summary aggregates terminal runs within the 30-day window.
#[tokio::test]
async fn test_cost_summary_aggregates_terminal_runs() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-agg-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Seed 5 terminal runs (mix Completed/Failed/Killed) within the last 30 days.
    let statuses = [
        RunStatus::Completed,
        RunStatus::Failed,
        RunStatus::Killed,
        RunStatus::Completed,
        RunStatus::Failed,
    ];
    for status in statuses {
        insert_terminal_run(
            &run_store,
            wf_id,
            &wf,
            status,
            Utc::now() - Duration::days(5),
            Some(0.10),
        )
        .await;
    }

    let resp: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}")
        .json()
        .await
        .unwrap();

    let cs = &resp["cost_summary"];
    let total = cs["last_30_days_total_usd"].as_f64().expect("f64");
    assert!(
        (total - 0.50).abs() < 1e-9,
        "last_30_days_total_usd should be ~0.50, got {}",
        total
    );
    assert_eq!(cs["last_30_days_runs"], 5);
}

/// CS-4. cost_summary excludes Running (non-terminal) runs.
#[tokio::test]
async fn test_cost_summary_excludes_running_runs() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-excl-running-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // 3 Completed runs + 2 Running runs.
    for _ in 0..3 {
        insert_terminal_run(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Completed,
            Utc::now() - Duration::days(2),
            Some(0.10),
        )
        .await;
    }
    for _ in 0..2 {
        insert_running_run(&run_store, wf_id, &wf).await;
    }

    let resp: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}")
        .json()
        .await
        .unwrap();

    let cs = &resp["cost_summary"];
    assert_eq!(
        cs["last_30_days_runs"], 3,
        "only terminal runs should count"
    );
    let total = cs["last_30_days_total_usd"].as_f64().expect("f64");
    assert!(
        (total - 0.30).abs() < 1e-9,
        "total should be ~0.30, got {}",
        total
    );
}

/// CS-5. cost_summary respects 30-day vs 1-year window boundaries.
#[tokio::test]
async fn test_cost_summary_excludes_runs_outside_window() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-window-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // 3 runs from 60 days ago (outside 30-day window but inside 1-year).
    for _ in 0..3 {
        insert_terminal_run(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Completed,
            Utc::now() - Duration::days(60),
            Some(0.10),
        )
        .await;
    }
    // 2 runs from 5 days ago (inside both windows).
    for _ in 0..2 {
        insert_terminal_run(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Completed,
            Utc::now() - Duration::days(5),
            Some(0.10),
        )
        .await;
    }

    let resp: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}")
        .json()
        .await
        .unwrap();

    let cs = &resp["cost_summary"];

    // 30-day window: only the 2 recent runs.
    assert_eq!(cs["last_30_days_runs"], 2);
    let total_30 = cs["last_30_days_total_usd"].as_f64().expect("f64");
    assert!(
        (total_30 - 0.20).abs() < 1e-9,
        "30-day total should be ~0.20, got {}",
        total_30
    );

    // 1-year window: all 5 runs.
    assert_eq!(cs["last_year_runs"], 5);
    let total_yr = cs["last_year_total_usd"].as_f64().expect("f64");
    assert!(
        (total_yr - 0.50).abs() < 1e-9,
        "1-year total should be ~0.50, got {}",
        total_yr
    );
}

/// CS-6. cost_summary handles null total_cost_usd: run is counted but not summed.
#[tokio::test]
async fn test_cost_summary_handles_null_cost_in_sum() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-null-cost-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // 3 Killed runs with null cost.
    for _ in 0..3 {
        insert_terminal_run(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Killed,
            Utc::now() - Duration::days(3),
            None,
        )
        .await;
    }
    // 1 Completed run with $0.50.
    insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(0.50),
    )
    .await;

    let resp: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}")
        .json()
        .await
        .unwrap();

    let cs = &resp["cost_summary"];
    // All 4 runs counted regardless of null cost.
    assert_eq!(cs["last_30_days_runs"], 4);
    let total = cs["last_30_days_total_usd"].as_f64().expect("f64");
    assert!(
        (total - 0.50).abs() < 1e-9,
        "total should be ~0.50 (null costs skipped), got {}",
        total
    );
}

/// CS-7. cost_summary cache invalidates when a new terminal run is added.
#[tokio::test]
async fn test_cost_summary_cache_invalidates_on_run_completion() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create a workflow with a fast echo step.
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-invalidate-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Seed one initial terminal run with $0.10.
    insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(0.10),
    )
    .await;

    // First GET — should show 1 run, $0.10.
    let resp1: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id} 1")
        .json()
        .await
        .unwrap();
    let cs1 = &resp1["cost_summary"];
    assert_eq!(cs1["last_30_days_runs"], 1);
    let total1 = cs1["last_30_days_total_usd"].as_f64().unwrap();
    assert!(
        (total1 - 0.10).abs() < 1e-9,
        "initial total should be ~0.10"
    );

    // Insert a second terminal run.
    insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        Utc::now() - Duration::hours(1),
        Some(0.10),
    )
    .await;

    // Invalidate the cache entry to simulate what the production cache-invalidation
    // path does when it receives a RunCompleted/RunFailed event.
    state.cost_cache.forget(wf_id).await;

    // Poll until the second GET reflects the new run (with a small retry loop in
    // case of any residual propagation latency).
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(2);
    let final_runs: u64 = loop {
        let resp: serde_json::Value = client
            .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
            .send()
            .await
            .expect("GET /api/cost/workflows/{id} 2")
            .json()
            .await
            .unwrap();
        let runs = resp["cost_summary"]["last_30_days_runs"]
            .as_u64()
            .unwrap_or(0);
        if runs >= 2 {
            break runs;
        }
        if tokio::time::Instant::now() >= deadline {
            break runs;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    assert_eq!(
        final_runs, 2,
        "cache should reflect the second run after invalidation"
    );
}

/// CS-8. cost_summary cache is evicted on workflow delete; re-created workflow
///       starts with a fresh (zero) cost summary.
#[tokio::test]
async fn test_cost_summary_cache_evicted_on_workflow_delete() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create workflow and seed a run to populate the cache.
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-evict-wf")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    let run_id = insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(1.00),
    )
    .await;

    // GET to populate cache via the cost endpoint.
    let resp1: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id} before delete")
        .json()
        .await
        .unwrap();
    assert_eq!(resp1["cost_summary"]["last_30_days_runs"], 1);

    // Delete the child run first to satisfy the workflow_runs.workflow_id FK,
    // then DELETE the workflow.
    run_store
        .delete_run(run_id)
        .await
        .expect("delete child run");
    let del_resp = client
        .delete(format!("{}/api/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(del_resp.status(), 204);

    // Subsequent GET returns 404.
    let get_resp = client
        .get(format!("{}/api/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET after delete");
    assert_eq!(get_resp.status(), 404);

    // Re-create a workflow with the same name — it gets a new UUID and fresh
    // (zero) cost summary; no leftover state from the deleted workflow.
    let recreated: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("cs-evict-wf")).unwrap())
        .send()
        .await
        .expect("POST recreate")
        .json()
        .await
        .unwrap();
    assert_ne!(
        recreated["id"].as_str().unwrap(),
        wf_id_str,
        "recreated workflow should have a new UUID"
    );

    let resp2: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}",
            base_url,
            recreated["id"].as_str().unwrap()
        ))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id} recreated")
        .json()
        .await
        .unwrap();
    assert_eq!(
        resp2["cost_summary"]["last_30_days_runs"], 0,
        "recreated workflow should have zero runs in cost_summary"
    );
}

// ---------------------------------------------------------------------------

/// 20. Restart simulation: runs persisted by instance A are readable by instance B
///     pointing at the same data_dir.
#[tokio::test]
async fn test_restart_simulation_run_persists() {
    let tmp = tempfile::TempDir::new().expect("create temp dir");
    let data_dir = tmp.path().to_path_buf();

    let run_id_str;

    // Instance A — create a workflow and trigger it.
    {
        let db_a = agent_cron_scheduler::storage::sqlite::init_db(&data_dir.join("acs.db"))
            .expect("init SQLite (instance A)");
        let wf_store_a = agent_cron_scheduler::storage::sqlite::SqliteWorkflowStore::new(&db_a);
        let run_store_a = agent_cron_scheduler::storage::sqlite::SqliteWorkflowRunStore::new(&db_a);
        let (base_url, _state, _handle) = spawn_test_server(
            Arc::new(wf_store_a) as Arc<dyn WorkflowStore>,
            Arc::new(run_store_a) as Arc<dyn WorkflowRunStore>,
            data_dir.clone(),
        )
        .await;
        let client = reqwest::Client::new();

        let created: serde_json::Value = client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow("restart-test")).unwrap())
            .send()
            .await
            .expect("POST")
            .json()
            .await
            .unwrap();
        let wf_id = created["id"].as_str().unwrap();

        let trigger_json: serde_json::Value = client
            .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
            .header("Content-Type", "application/json")
            .body(r#"{"input": null}"#)
            .send()
            .await
            .expect("trigger")
            .json()
            .await
            .unwrap();
        run_id_str = trigger_json["run_id"].as_str().unwrap().to_string();

        // Wait for run to complete before "restarting".
        let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
        loop {
            if tokio::time::Instant::now() >= deadline {
                panic!("Run did not complete within 10s before restart simulation");
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
            let run_resp = client
                .get(format!("{}/api/runs/{}", base_url, run_id_str))
                .send()
                .await
                .expect("GET run");
            if run_resp.status() == 200 {
                let run_json: serde_json::Value = run_resp.json().await.unwrap();
                let status = run_json["status"].as_str().unwrap_or("");
                if status == "Completed" || status == "Failed" {
                    break;
                }
            }
        }
    }

    // Instance B — open the same data_dir and verify the run is readable.
    {
        let db_b = agent_cron_scheduler::storage::sqlite::init_db(&data_dir.join("acs.db"))
            .expect("init SQLite (instance B)");
        let wf_store_b = agent_cron_scheduler::storage::sqlite::SqliteWorkflowStore::new(&db_b);
        let run_store_b = agent_cron_scheduler::storage::sqlite::SqliteWorkflowRunStore::new(&db_b);
        let (base_url_b, _state_b, _handle_b) = spawn_test_server(
            Arc::new(wf_store_b) as Arc<dyn WorkflowStore>,
            Arc::new(run_store_b) as Arc<dyn WorkflowRunStore>,
            data_dir.clone(),
        )
        .await;
        let client = reqwest::Client::new();

        let run_resp = client
            .get(format!("{}/api/runs/{}", base_url_b, run_id_str))
            .send()
            .await
            .expect("GET run on instance B");

        assert_eq!(
            run_resp.status(),
            200,
            "Run should be readable from a fresh store instance (restart simulation)"
        );
        let run_json: serde_json::Value = run_resp.json().await.unwrap();
        assert_eq!(
            run_json["run_id"].as_str().unwrap(),
            run_id_str,
            "run_id should match"
        );
    }
}

// ---------------------------------------------------------------------------
// ACS-18 API correctness tests
// ---------------------------------------------------------------------------

/// Trigger response includes run_url formatted as /api/runs/{run_id}.
#[tokio::test]
async fn test_trigger_response_includes_run_url() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("run-url-test")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();

    let run_id = trigger_json["run_id"].as_str().expect("run_id present");
    let run_url = trigger_json["run_url"].as_str().expect("run_url present");
    assert_eq!(
        run_url,
        format!("/api/runs/{}", run_id),
        "run_url should be /api/runs/{{run_id}}"
    );
}

/// POST /api/runs/{run_id}/kill returns 202 with JSON body {"message": "Kill signal sent"}.
#[tokio::test]
async fn test_kill_endpoint_returns_json_body() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    #[cfg(windows)]
    let sleep_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
    #[cfg(not(windows))]
    let sleep_cmd = "sleep 30";

    let wf = NewWorkflow {
        name: "kill-body-test".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![shell_step("slow", sleep_cmd)],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap();

    // Give the executor a moment to start.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let kill_resp = client
        .post(format!("{}/api/runs/{}/kill", base_url, run_id))
        .send()
        .await
        .expect("POST /kill");
    assert_eq!(kill_resp.status(), 202, "kill should return 202");

    let kill_json: serde_json::Value = kill_resp
        .json()
        .await
        .expect("kill response should be JSON");
    assert_eq!(
        kill_json["message"], "Kill signal sent",
        "kill body should contain message"
    );
}

/// POST /api/workflows/{id}/trigger with empty body `{}` succeeds (input defaults to null).
#[tokio::test]
async fn test_trigger_with_empty_body_succeeds() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("empty-body-trigger")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let resp = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{}"#)
        .send()
        .await
        .expect("trigger with empty body");

    assert_eq!(
        resp.status(),
        202,
        "triggering with {{}} body should succeed (input defaults to null)"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(json["run_id"].is_string(), "run_id should be present");
}

// ---------------------------------------------------------------------------
// last_run_* propagation tests
//
// Regression coverage for the ACS-22 follow-up: after a run reaches a terminal
// status, the parent workflow row's `last_run_id`, `last_run_status`, and
// `last_run_at` fields must be populated so they show up in
// GET /api/workflows[/{id}] responses.
// ---------------------------------------------------------------------------

/// Helper: poll GET /api/workflows/{id} until last_run_id is non-null (or
/// `deadline_secs` elapses). Returns the workflow JSON.
async fn poll_for_last_run(
    client: &reqwest::Client,
    base_url: &str,
    wf_id: &str,
    deadline_secs: u64,
) -> serde_json::Value {
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(deadline_secs);
    loop {
        if tokio::time::Instant::now() >= deadline {
            panic!(
                "Workflow {} did not get a last_run_id within {}s",
                wf_id, deadline_secs
            );
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let wf: serde_json::Value = client
            .get(format!("{}/api/workflows/{}", base_url, wf_id))
            .send()
            .await
            .expect("GET workflow")
            .json()
            .await
            .unwrap();
        if wf["last_run_id"].is_string() {
            return wf;
        }
    }
}

/// Successful run: last_run_status should be "Completed" and last_run_id
/// should match the run's id.
#[tokio::test]
async fn test_successful_run_populates_workflow_last_run_fields() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("last-run-success")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap().to_string();
    assert!(
        created["last_run_id"].is_null(),
        "fresh workflow must start with last_run_id=null"
    );
    assert!(created["last_run_status"].is_null());
    assert!(created["last_run_at"].is_null());

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap().to_string();

    let wf = poll_for_last_run(&client, &base_url, &wf_id, 10).await;
    assert_eq!(
        wf["last_run_id"].as_str(),
        Some(run_id.as_str()),
        "workflow.last_run_id should match the triggered run"
    );
    assert_eq!(
        wf["last_run_status"].as_str(),
        Some("Completed"),
        "workflow.last_run_status should be Completed for a successful run"
    );
    assert!(
        wf["last_run_at"].is_string(),
        "workflow.last_run_at should be set"
    );
}

/// Failing run (non-zero exit, default Abort policy): last_run_status="Failed".
#[tokio::test]
async fn test_failed_run_populates_workflow_last_run_fields() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Platform-appropriate "exit 1" command.
    #[cfg(windows)]
    let fail_cmd = "cmd /C exit 1";
    #[cfg(not(windows))]
    let fail_cmd = "false";

    let wf = NewWorkflow {
        name: "last-run-fail".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![shell_step("boom", fail_cmd)],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap().to_string();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap().to_string();

    let got = poll_for_last_run(&client, &base_url, &wf_id, 10).await;
    assert_eq!(got["last_run_id"].as_str(), Some(run_id.as_str()));
    assert_eq!(
        got["last_run_status"].as_str(),
        Some("Failed"),
        "non-zero exit with default Abort policy should mark workflow.last_run_status=Failed"
    );
}

/// Killed run: last_run_status="Killed".
#[tokio::test]
async fn test_killed_run_populates_workflow_last_run_fields() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    #[cfg(windows)]
    let sleep_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
    #[cfg(not(windows))]
    let sleep_cmd = "sleep 30";

    let wf = NewWorkflow {
        name: "last-run-kill".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![shell_step("slow", sleep_cmd)],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap().to_string();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap().to_string();

    // Let the executor settle on the long-sleep step.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let kill_resp = client
        .post(format!("{}/api/runs/{}/kill", base_url, run_id))
        .send()
        .await
        .expect("POST /kill");
    assert_eq!(kill_resp.status(), 202);

    let got = poll_for_last_run(&client, &base_url, &wf_id, 10).await;
    assert_eq!(got["last_run_id"].as_str(), Some(run_id.as_str()));
    assert_eq!(
        got["last_run_status"].as_str(),
        Some("Killed"),
        "killed run should propagate Killed to workflow.last_run_status"
    );
}

// ---------------------------------------------------------------------------
// ACS-22 v4.2.4 follow-ups
// ---------------------------------------------------------------------------

/// Regression: test fixtures must NOT create real folders under the user's
/// Documents directory. When `display_workflow_dir_root` is set on the test
/// daemon config, auto-created workflow dirs land there (under the tempdir).
#[tokio::test]
async fn test_default_working_dir_routed_to_config_root_not_documents() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let tmp_path = tmp.path().to_path_buf();
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp_path.clone(),
    )
    .await;
    let client = reqwest::Client::new();

    // Use a recognisable, unique workflow name so we can assert on its
    // sanitised directory.
    let name = format!("doc-dir-regression-{}", Uuid::now_v7());
    let body = serde_json::to_string(&make_new_workflow(&name)).unwrap();
    let resp = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST /api/workflows");
    assert_eq!(resp.status(), 201);
    let json: serde_json::Value = resp.json().await.unwrap();
    let working_dir = json["working_dir"]
        .as_str()
        .expect("working_dir should be auto-filled");

    // The auto-created path must live under <tempdir>/workflow_dirs/, not
    // under the user's real Documents folder.
    let expected_root = tmp_path.join("workflow_dirs");
    let wd_path = std::path::Path::new(working_dir);
    assert!(
        wd_path.starts_with(&expected_root),
        "working_dir {:?} must live under the test-configured root {:?}",
        wd_path,
        expected_root
    );
    assert!(wd_path.exists(), "working_dir should be created on disk");

    // Hard assertion: nothing should have been created under the real
    // Documents/agent-cron-scheduler/<sanitised-name>/ path.
    if let Some(docs) = dirs::document_dir() {
        let leaked = docs
            .join("agent-cron-scheduler")
            .join(name.replace('_', "-").to_ascii_lowercase());
        assert!(
            !leaked.exists(),
            "test must not create a directory under the user's Documents folder: {:?}",
            leaked
        );
    }
}

/// Item 3c: when `create_dir_all` fails for the auto-created working_dir, the
/// API must return 422 `working_dir_create_failed` rather than persisting the
/// workflow with a silent `working_dir = null`.
#[tokio::test]
async fn test_create_workflow_422_when_default_dir_unwriteable() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let tmp_path = tmp.path().to_path_buf();
    // Stage the root as a regular FILE so create_dir_all fails on every
    // platform (it can't create a directory inside a file).
    let blocking_root = tmp_path.join("workflow_dirs");
    std::fs::write(&blocking_root, b"not a directory").expect("create blocking file");

    let (base_url, _state, _handle) =
        spawn_test_server(Arc::clone(&wf_store), Arc::clone(&run_store), tmp_path).await;
    let client = reqwest::Client::new();

    let body = serde_json::to_string(&make_new_workflow("dir-fail")).unwrap();
    let resp = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(body)
        .send()
        .await
        .expect("POST /api/workflows");
    assert_eq!(
        resp.status(),
        422,
        "create_dir_all failure must surface as 422, not silent null"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        json["error"], "working_dir_create_failed",
        "error code should match"
    );

    // Verify the workflow was NOT persisted.
    // v4.2.10: response is a plain array (cost data moved to /api/cost/workflows).
    let list_resp = client
        .get(format!("{}/api/workflows", base_url))
        .send()
        .await
        .expect("GET")
        .json::<serde_json::Value>()
        .await
        .unwrap();
    assert!(
        list_resp.as_array().unwrap().is_empty(),
        "workflow must not be persisted when default_dir creation fails"
    );
}

/// Item 3b: `read_step_slice` must surface a 422 `log_offset_out_of_range`
/// when `log_byte_offset_start` is beyond the log file length. Previously the
/// endpoint silently returned an empty body.
#[tokio::test]
async fn test_run_log_slice_422_when_start_offset_past_eof() {
    use agent_cron_scheduler::models::workflow::{RunStatus, StepRun, WorkflowRun};

    let (wf_store, run_store, tmp) = make_stores().await;
    let tmp_path = tmp.path().to_path_buf();
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp_path.clone(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create a workflow so we have a valid workflow_id to attach the run to.
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("slice-oob")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Build a synthetic run whose only StepRun records a start offset far
    // beyond the actual log file size.
    let run_id = uuid::Uuid::now_v7();
    let log_dir = tmp_path.join("logs").join(wf_id.to_string());
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    let log_path = log_dir.join(format!("{}.log", run_id));
    // Write a small log file: ~16 bytes.
    std::fs::write(&log_path, b"tiny log body\n").expect("write log");

    let step_run = StepRun {
        step_index: 0,
        step_id: "phantom".to_string(),
        kind: "shell".to_string(),
        status: RunStatus::Failed,
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        exit_code: None,
        log_byte_offset_start: 999_999, // way past file_len
        log_byte_offset_end: None,
        cost_usd: None,
        error: Some("synthetic".to_string()),
    };

    let run = WorkflowRun {
        run_id,
        workflow_id: wf_id,
        workflow_version: wf.version,
        workflow_snapshot: wf.clone(),
        started_at: Utc::now(),
        finished_at: Some(Utc::now()),
        status: RunStatus::Failed,
        trigger_input: None,
        steps: vec![step_run],
        total_cost_usd: None,
        total_duration_ms: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
    };
    run_store.create_run(run).await.expect("create_run");

    // Hit the slice endpoint with step_index=0; expect 422.
    let resp = client
        .get(format!("{}/api/runs/{}/log?step_index=0", base_url, run_id))
        .send()
        .await
        .expect("GET run log slice");
    assert_eq!(
        resp.status(),
        422,
        "start offset past EOF must surface as 422 log_offset_out_of_range"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(json["error"], "log_offset_out_of_range");
}

// ---------------------------------------------------------------------------
// AgentStep.command_template rejection tests (v4.2.7)
// ---------------------------------------------------------------------------

/// POST /api/workflows with an AgentStep that includes the removed
/// `command_template` field must return 400 command_template_removed.
#[tokio::test]
async fn test_post_workflow_with_command_template_returns_400() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "name": "test-ct-post",
        "schedule": "*/5 * * * *",
        "steps": [
            {
                "kind": "agent",
                "id": "a1",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "command_template": "claude -p \"${prompt}\""
            }
        ]
    });

    let resp = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .expect("POST");
    assert_eq!(resp.status(), 400, "expected 400 for command_template");
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        json["error"].as_str().unwrap_or(""),
        "command_template_removed"
    );
    assert!(
        json["message"]
            .as_str()
            .unwrap_or("")
            .contains("command_template"),
        "message should mention command_template: {:?}",
        json["message"]
    );
}

/// PATCH /api/workflows/{id} with steps containing `command_template`
/// must return 400.
#[tokio::test]
async fn test_patch_workflow_with_command_template_returns_400() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // First, create a valid workflow.
    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("ct-patch-test")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    // PATCH with an agent step containing command_template.
    let patch_body = serde_json::json!({
        "steps": [
            {
                "kind": "agent",
                "id": "a1",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "command_template": "claude -p \"${prompt}\""
            }
        ]
    });
    let resp = client
        .patch(format!("{}/api/workflows/{}", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&patch_body).unwrap())
        .send()
        .await
        .expect("PATCH");
    assert_eq!(
        resp.status(),
        400,
        "expected 400 for command_template on PATCH"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        json["error"].as_str().unwrap_or(""),
        "command_template_removed"
    );
}

/// POST /api/workflows with a valid AgentStep using the new `model` and
/// `extra_args` fields must be accepted (201).
#[tokio::test]
async fn test_post_workflow_with_agent_step_model_and_extra_args_accepted() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let body = serde_json::json!({
        "name": "test-agent-new-shape",
        "schedule": "*/5 * * * *",
        "steps": [
            {
                "kind": "agent",
                "id": "a1",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "model": "claude-opus-4-5",
                "extra_args": ["--resume", "session-id"]
            }
        ]
    });

    let resp = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .expect("POST");
    assert_eq!(
        resp.status(),
        201,
        "new agent step shape should be accepted"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    // Verify the model and extra_args are persisted correctly.
    let steps = json["steps"].as_array().expect("steps");
    let agent_step = &steps[0];
    assert_eq!(
        agent_step["model"].as_str().unwrap_or(""),
        "claude-opus-4-5"
    );
    let extra = agent_step["extra_args"].as_array().expect("extra_args");
    assert_eq!(extra[0].as_str().unwrap_or(""), "--resume");
    assert_eq!(extra[1].as_str().unwrap_or(""), "session-id");
}

/// PATCH /api/workflows/{id} with a valid agent step shape (model + extra_args)
/// must be accepted (200).
#[tokio::test]
async fn test_patch_workflow_with_agent_step_new_shape_accepted() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("ct-patch-valid")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let patch_body = serde_json::json!({
        "steps": [
            {
                "kind": "agent",
                "id": "a1",
                "agent_type": "claude_code_cli",
                "prompt": "hello",
                "model": "claude-sonnet-4-5",
                "extra_args": []
            }
        ]
    });
    let resp = client
        .patch(format!("{}/api/workflows/{}", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&patch_body).unwrap())
        .send()
        .await
        .expect("PATCH");
    assert_eq!(
        resp.status(),
        200,
        "new agent step shape should be accepted on PATCH"
    );
}

/// Item 2: a step that times out must record `log_byte_offset_start` at the
/// offset of its START marker (not 0) so the slice endpoint returns the
/// step's bytes rather than the whole log file.
///
/// We use a 2-step workflow: step-1 writes a known prefix to the log, step-2
/// runs a sleep with a tight `timeout_secs` so it times out. After the run
/// finishes, step-2's StepRun must carry a non-zero `log_byte_offset_start`
/// (the byte offset just before its START marker).
#[tokio::test]
async fn test_timed_out_step_records_log_byte_offset_start() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let tmp_path = tmp.path().to_path_buf();
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp_path.clone(),
    )
    .await;
    let client = reqwest::Client::new();

    #[cfg(windows)]
    let prefix_cmd = "echo PREFIX-MARKER";
    #[cfg(not(windows))]
    let prefix_cmd = "echo PREFIX-MARKER";

    #[cfg(windows)]
    let slow_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
    #[cfg(not(windows))]
    let slow_cmd = "sleep 30";

    let prefix_step = StepDef::Shell(ShellStep {
        common: StepDefCommon {
            id: "prefix".to_string(),
            on_failure: None,
            always_run: false,
            timeout_secs: Some(30),
            working_dir: None,
            env_vars: None,
            capture: CaptureSpec::default(),
        },
        command: prefix_cmd.to_string(),
        pass_stdin: false,
    });
    let slow_step = StepDef::Shell(ShellStep {
        common: StepDefCommon {
            id: "slow".to_string(),
            on_failure: None,
            always_run: false,
            timeout_secs: Some(2), // tight timeout → step times out
            working_dir: None,
            env_vars: None,
            capture: CaptureSpec::default(),
        },
        command: slow_cmd.to_string(),
        pass_stdin: false,
    });
    let wf = NewWorkflow {
        name: "timeout-offset-capture".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![prefix_step, slow_step],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap();

    // Poll until the run finishes (should be ~2-3s).
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(15);
    let final_run = loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("run {} did not finish within 15s", run_id);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let run_resp = client
            .get(format!("{}/api/runs/{}", base_url, run_id))
            .send()
            .await
            .expect("GET run");
        if run_resp.status() == 200 {
            let run_json: serde_json::Value = run_resp.json().await.unwrap();
            let status = run_json["status"].as_str().unwrap_or("");
            if matches!(status, "Failed" | "Completed" | "Killed") {
                break run_json;
            }
        }
    };

    // Locate the slow step (step_index 1).
    let steps = final_run["steps"].as_array().expect("steps array");
    let slow = steps
        .iter()
        .find(|s| s["step_id"] == "slow")
        .expect("slow step recorded");

    let start = slow["log_byte_offset_start"]
        .as_u64()
        .expect("log_byte_offset_start present");
    assert!(
        start > 0,
        "timed-out step must capture its START-marker offset; got {} (would have been 0 before the fix)",
        start
    );
    // The step impl writes the END marker on the timeout path before
    // surfacing `StepError::Timeout`, so the executor stamps that offset
    // onto the failed StepRun. `end` must be `Some(N)` with `N > start`.
    let end = slow["log_byte_offset_end"].as_u64().expect(
        "log_byte_offset_end must be Some(N) — the END marker is written on the timeout path",
    );
    assert!(
        end > start,
        "end offset ({}) must exceed start offset ({})",
        end,
        start
    );

    // Sanity: GET /api/runs/{id}/log?step_index=1 should return bytes
    // starting from the slow step's START marker — must NOT contain the
    // prefix step's "PREFIX-MARKER" output.
    let slice_resp = client
        .get(format!("{}/api/runs/{}/log?step_index=1", base_url, run_id))
        .send()
        .await
        .expect("GET slice");
    assert_eq!(slice_resp.status(), 200);
    let body = slice_resp.text().await.expect("read body");
    assert!(
        !body.contains("PREFIX-MARKER"),
        "step slice must not include the prior step's output. body={:?}",
        body
    );
}

// ---------------------------------------------------------------------------
// Daily cost buckets — v4.2.9
// ---------------------------------------------------------------------------

/// Helper: insert a terminal run with an explicit `finished_at` date string
/// (YYYY-MM-DD interpreted as UTC midnight) and a per-status cost.
async fn insert_dated_run(
    run_store: &Arc<dyn WorkflowRunStore>,
    workflow_id: uuid::Uuid,
    wf: &agent_cron_scheduler::models::workflow::Workflow,
    status: RunStatus,
    date_utc: &str,
    total_cost_usd: Option<f64>,
) -> uuid::Uuid {
    let finished_at = chrono::DateTime::parse_from_rfc3339(&format!("{}T12:00:00Z", date_utc))
        .expect("parse date")
        .with_timezone(&Utc);
    insert_terminal_run(
        run_store,
        workflow_id,
        wf,
        status,
        finished_at,
        total_cost_usd,
    )
    .await
}

/// DB-1. GET /api/cost/workflows returns a wrapped CostWorkflowsListResponse.
/// (v4.2.10: cost data relocated from /api/workflows to /api/cost/workflows)
/// Verifies {workflows: Vec<WorkflowCostEntry>, system_cost_summary} shape.
#[tokio::test]
async fn test_cost_list_response_is_wrapped() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    for name in ["db1-wf-a", "db1-wf-b"] {
        client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow(name)).unwrap())
            .send()
            .await
            .expect("POST workflow")
            .error_for_status()
            .expect("201 created");
    }

    let resp = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET /api/cost/workflows");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    // Root must be an object with `workflows` + `system_cost_summary`.
    assert!(json.is_object(), "response must be an object");

    // `workflows` field must contain WorkflowCostEntry objects for both workflows.
    let workflows = json["workflows"].as_array().expect("workflows is array");
    assert_eq!(workflows.len(), 2, "expected 2 cost entries");
    let names: Vec<&str> = workflows
        .iter()
        .map(|w| w["workflow_name"].as_str().unwrap())
        .collect();
    assert!(names.contains(&"db1-wf-a"), "db1-wf-a should be present");
    assert!(names.contains(&"db1-wf-b"), "db1-wf-b should be present");

    // `system_cost_summary` must be present with all 5 baseline fields.
    let scs = &json["system_cost_summary"];
    assert!(!scs.is_null(), "system_cost_summary must be present");
    assert!(scs["last_30_days_total_usd"].is_number());
    assert!(scs["last_30_days_runs"].is_number());
    assert!(scs["last_year_total_usd"].is_number());
    assert!(scs["last_year_runs"].is_number());
    assert!(scs["computed_at"].is_string());

    // `daily_buckets` must be an array on the system summary.
    assert!(
        scs["daily_buckets"].is_array(),
        "system_cost_summary.daily_buckets must be an array"
    );
}

/// DB-2. GET /api/cost/workflows/{id} returns a CostWorkflowResponse (with daily_buckets).
/// (v4.2.10: cost data relocated from /api/workflows/{id} to /api/cost/workflows/{id})
#[tokio::test]
async fn test_cost_workflow_singular_response() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db2-singular")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}")
        .json()
        .await
        .unwrap();

    // Must be a CostWorkflowResponse: { workflow_id, workflow_name, cost_summary }.
    assert!(
        json["workflow_id"].is_string(),
        "CostWorkflowResponse should have workflow_id at root"
    );
    assert!(
        json["workflow_name"].is_string(),
        "CostWorkflowResponse should have workflow_name at root"
    );

    // `cost_summary.daily_buckets` must exist as an array.
    let cs = &json["cost_summary"];
    assert!(!cs.is_null(), "cost_summary must be present");
    assert!(
        cs["daily_buckets"].is_array(),
        "cost_summary.daily_buckets must be an array on singular cost GET"
    );
}

/// DB-3. Daily buckets aggregate Completed runs on the same day.
#[tokio::test]
async fn test_daily_buckets_aggregate_completed() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db3-completed")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // 3 Completed runs yesterday ($0.10 each).
    // Use yesterday (UTC-1d) so the run falls safely inside the 30-day window
    // regardless of the UTC-to-display-timezone offset at test runtime.
    let today = (Utc::now() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    for _ in 0..3 {
        insert_dated_run(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Completed,
            &today,
            Some(0.10),
        )
        .await;
    }

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets is array");
    assert_eq!(
        buckets.len(),
        1,
        "exactly 1 bucket for runs all on the same day"
    );

    let b = &buckets[0];
    let total = b["total_usd"].as_f64().expect("total_usd");
    assert!(
        (total - 0.30).abs() < 1e-9,
        "total_usd should be ~0.30, got {}",
        total
    );
    let cost_completed = b["cost_from_completed"]
        .as_f64()
        .expect("cost_from_completed");
    assert!(
        (cost_completed - 0.30).abs() < 1e-9,
        "cost_from_completed should be ~0.30, got {}",
        cost_completed
    );
    assert_eq!(b["runs_completed"], 3, "runs_completed should be 3");
    assert_eq!(
        b["cost_from_failed"].as_f64().unwrap_or(0.0),
        0.0,
        "cost_from_failed should be 0"
    );
    assert_eq!(
        b["cost_from_killed"].as_f64().unwrap_or(0.0),
        0.0,
        "cost_from_killed should be 0"
    );
    assert_eq!(
        b["runs_failed"].as_u64().unwrap_or(0),
        0,
        "runs_failed should be 0"
    );
    assert_eq!(
        b["runs_killed"].as_u64().unwrap_or(0),
        0,
        "runs_killed should be 0"
    );
}

/// DB-4. Status breakdown: Completed/Failed/Killed attributed to right fields.
#[tokio::test]
async fn test_daily_buckets_status_breakdown() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db4-breakdown")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Use yesterday (UTC-1d) so the run falls safely inside the 30-day window
    // regardless of the UTC-to-display-timezone offset at test runtime.
    let today = (Utc::now() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    // 2 Completed ($0.10 each), 1 Failed ($0.20), 1 Killed (null cost).
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &today,
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &today,
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Failed,
        &today,
        Some(0.20),
    )
    .await;
    insert_dated_run(&run_store, wf_id, &wf, RunStatus::Killed, &today, None).await;

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");
    assert_eq!(buckets.len(), 1, "all runs on same day → 1 bucket");

    let b = &buckets[0];
    assert_eq!(b["runs_completed"], 2);
    assert_eq!(b["runs_failed"], 1);
    assert_eq!(b["runs_killed"], 1);
    let cost_c = b["cost_from_completed"]
        .as_f64()
        .expect("cost_from_completed");
    assert!(
        (cost_c - 0.20).abs() < 1e-9,
        "cost_from_completed ~0.20, got {}",
        cost_c
    );
    let cost_f = b["cost_from_failed"].as_f64().expect("cost_from_failed");
    assert!(
        (cost_f - 0.20).abs() < 1e-9,
        "cost_from_failed ~0.20, got {}",
        cost_f
    );
    let cost_k = b["cost_from_killed"].as_f64().expect("cost_from_killed");
    assert!(
        cost_k.abs() < 1e-9,
        "cost_from_killed ~0.0 (null cost), got {}",
        cost_k
    );
}

/// DB-5. Zero-cost days are omitted — buckets only emitted for active days.
#[tokio::test]
async fn test_daily_buckets_omits_zero_days() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db5-sparse")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Runs on day-1, day-4, and day-7 within the last 30 days.
    let day1 = (Utc::now() - Duration::days(7))
        .format("%Y-%m-%d")
        .to_string();
    let day4 = (Utc::now() - Duration::days(4))
        .format("%Y-%m-%d")
        .to_string();
    let day7 = (Utc::now() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &day1,
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &day4,
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &day7,
        Some(0.10),
    )
    .await;

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");
    assert_eq!(
        buckets.len(),
        3,
        "exactly 3 active days should produce 3 buckets (no zero-padding)"
    );
}

/// DB-6. Buckets are returned in ascending date order.
#[tokio::test]
async fn test_daily_buckets_ascending_order() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db6-order")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Insert in non-chronological order.
    let day_newest = (Utc::now() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    let day_oldest = (Utc::now() - Duration::days(10))
        .format("%Y-%m-%d")
        .to_string();
    let day_mid = (Utc::now() - Duration::days(5))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &day_newest,
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &day_oldest,
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &day_mid,
        Some(0.10),
    )
    .await;

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");
    assert_eq!(buckets.len(), 3, "3 distinct days → 3 buckets");

    // Dates must be strictly ascending.
    let dates: Vec<&str> = buckets
        .iter()
        .map(|b| b["date"].as_str().unwrap())
        .collect();
    assert!(
        dates[0] < dates[1] && dates[1] < dates[2],
        "buckets must be sorted ascending by date; got {:?}",
        dates
    );
    assert_eq!(dates[0], day_oldest, "first bucket = oldest date");
    assert_eq!(dates[2], day_newest, "last bucket = newest date");
}

/// DB-7. system_cost_summary.daily_buckets aggregates across all workflows.
#[tokio::test]
async fn test_system_aggregate_sums_across_workflows() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create 2 workflows with runs on the same day.
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let mut per_wf_30d_totals: Vec<f64> = vec![];

    for name in ["db7-wf-a", "db7-wf-b"] {
        let created: serde_json::Value = client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow(name)).unwrap())
            .send()
            .await
            .expect("POST")
            .json()
            .await
            .unwrap();
        let wf_id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
        let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
        insert_dated_run(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Completed,
            &today,
            Some(0.15),
        )
        .await;
        per_wf_30d_totals.push(0.15);
    }

    let list_json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET /api/cost/workflows list")
        .json()
        .await
        .unwrap();

    // System summary daily_buckets should aggregate across both workflows.
    let scs = &list_json["system_cost_summary"];
    assert!(
        scs["daily_buckets"].is_array(),
        "system daily_buckets must be array"
    );
    let sys_buckets = scs["daily_buckets"].as_array().unwrap();
    // At minimum, today's bucket should exist with both workflows' costs.
    let today_bucket = sys_buckets
        .iter()
        .find(|b| b["date"].as_str() == Some(today.as_str()));
    if let Some(tb) = today_bucket {
        let sys_total = tb["total_usd"].as_f64().unwrap_or(0.0);
        assert!(
            sys_total >= 0.29,
            "system today bucket total_usd should be ~0.30 (sum of 2 workflows), got {}",
            sys_total
        );
    }

    // system_cost_summary.last_30_days_total_usd should equal sum of per-workflow totals.
    let per_wf_sum: f64 = per_wf_30d_totals.iter().sum();
    let sys_30d = scs["last_30_days_total_usd"].as_f64().unwrap_or(0.0);
    assert!(
        (sys_30d - per_wf_sum).abs() < 1e-9,
        "system last_30_days_total_usd ({}) should equal sum of per-workflow totals ({})",
        sys_30d,
        per_wf_sum
    );
}

/// DB-8. ?days=30 (default) — run from 35 days ago NOT in buckets.
#[tokio::test]
async fn test_query_days_30_default() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db8-default-window")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Run 35 days ago — outside the 30-day window.
    let old_day = (Utc::now() - Duration::days(35))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &old_day,
        Some(0.50),
    )
    .await;
    // Run 5 days ago — inside the window.
    let recent_day = (Utc::now() - Duration::days(5))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &recent_day,
        Some(0.10),
    )
    .await;

    let json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id} (no params → default days=30)")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");

    // Only the recent run should appear; the 35-day-old run must not be in any bucket.
    let has_old = buckets
        .iter()
        .any(|b| b["date"].as_str() == Some(old_day.as_str()));
    assert!(
        !has_old,
        "run from 35 days ago must not appear in the default 30-day bucket window"
    );
    let has_recent = buckets
        .iter()
        .any(|b| b["date"].as_str() == Some(recent_day.as_str()));
    assert!(
        has_recent,
        "run from 5 days ago must appear in the 30-day bucket window"
    );
}

/// DB-9. ?days=7 — runs from 8+ days ago excluded.
#[tokio::test]
async fn test_query_days_7() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db9-days7")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Run 8 days ago — outside the 7-day window.
    let old_day = (Utc::now() - Duration::days(8))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &old_day,
        Some(0.50),
    )
    .await;
    // Run 3 days ago — inside the 7-day window.
    let recent_day = (Utc::now() - Duration::days(3))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &recent_day,
        Some(0.10),
    )
    .await;

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=7",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET ?days=7")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");
    let has_old = buckets
        .iter()
        .any(|b| b["date"].as_str() == Some(old_day.as_str()));
    assert!(
        !has_old,
        "run from 8 days ago must not appear in ?days=7 window"
    );
    let has_recent = buckets
        .iter()
        .any(|b| b["date"].as_str() == Some(recent_day.as_str()));
    assert!(
        has_recent,
        "run from 3 days ago must appear in ?days=7 window"
    );
}

/// DB-10. ?since=...&until=... absolute window includes only runs in range.
#[tokio::test]
async fn test_query_since_until_absolute() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db10-abs-window")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Runs before, inside, and after the window 2026-04-01..2026-05-01.
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        "2026-03-15",
        Some(0.50),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        "2026-04-15",
        Some(0.10),
    )
    .await;
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        "2026-05-15",
        Some(0.25),
    )
    .await;

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?since=2026-04-01&until=2026-05-01",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET ?since=...&until=...")
        .json()
        .await
        .unwrap();

    let buckets = json["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");

    // Only the April 15 run should appear.
    assert_eq!(
        buckets.len(),
        1,
        "exactly 1 bucket expected for the run inside the window; got {:?}",
        buckets
            .iter()
            .map(|b| b["date"].as_str())
            .collect::<Vec<_>>()
    );
    assert_eq!(buckets[0]["date"].as_str().unwrap(), "2026-04-15");
}

/// DB-11. ?days=30&since=... returns 400 conflicting_params.
#[tokio::test]
async fn test_query_both_days_and_since_returns_400() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db11-conflict-params")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30&since=2026-01-01",
            base_url, wf_id
        ))
        .send()
        .await
        .expect("GET with conflicting params");
    assert_eq!(resp.status(), 400, "conflicting params should return 400");
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        json["error"].as_str().unwrap_or(""),
        "conflicting_params",
        "error code should be conflicting_params"
    );
}

/// DB-12. ?days=400 returns 400 window_too_large.
#[tokio::test]
async fn test_query_days_over_365_returns_400() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db12-days-too-large")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=400",
            base_url, wf_id
        ))
        .send()
        .await
        .expect("GET with days=400");
    assert_eq!(resp.status(), 400, "days > 365 should return 400");
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        json["error"].as_str().unwrap_or(""),
        "window_too_large",
        "error code should be window_too_large"
    );
}

/// DB-13. ?since= after ?until= returns 400 invalid_window.
#[tokio::test]
async fn test_query_since_after_until_returns_400() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db13-invalid-range")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/cost/workflows/{}?since=2026-05-01&until=2026-04-01",
            base_url, wf_id
        ))
        .send()
        .await
        .expect("GET since > until");
    assert_eq!(resp.status(), 400, "since > until should return 400");
    let json: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        json["error"].as_str().unwrap_or(""),
        "invalid_window",
        "error code should be invalid_window"
    );
}

/// DB-14. ?since= without ?until= returns 400 invalid_window.
#[tokio::test]
async fn test_query_only_since_no_until_returns_400() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db14-since-no-until")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let resp = client
        .get(format!(
            "{}/api/cost/workflows/{}?since=2026-04-01",
            base_url, wf_id
        ))
        .send()
        .await
        .expect("GET since without until");
    assert_eq!(resp.status(), 400, "since without until should return 400");
    let json: serde_json::Value = resp.json().await.unwrap();
    let code = json["error"].as_str().unwrap_or("");
    assert_eq!(
        code, "invalid_window",
        "error code should be invalid_window, got {:?}",
        code
    );
}

/// DB-15. Eager invalidation: after a new run completes, GET shows updated buckets.
#[tokio::test]
async fn test_daily_buckets_refresh_on_run_completion() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("db15-invalidation")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Seed 1 Completed run (yesterday — use UTC-1d so it falls safely inside
    // the 30-day window regardless of UTC-to-display-tz offset at runtime).
    let today = (Utc::now() - Duration::days(1))
        .format("%Y-%m-%d")
        .to_string();
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &today,
        Some(0.10),
    )
    .await;

    // Prime the cache with a GET — should show 1 bucket.
    let resp1: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET 1")
        .json()
        .await
        .unwrap();
    let buckets1 = resp1["cost_summary"]["daily_buckets"]
        .as_array()
        .expect("daily_buckets");
    let initial_runs = buckets1
        .iter()
        .find(|b| b["date"].as_str() == Some(today.as_str()))
        .map(|b| b["runs_completed"].as_u64().unwrap_or(0))
        .unwrap_or(0);
    assert_eq!(initial_runs, 1, "initial bucket should show 1 run");

    // Insert a second run then invalidate the cache entry (simulates event-bus path).
    insert_dated_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        &today,
        Some(0.10),
    )
    .await;
    state.cost_cache.forget(wf_id).await;

    // Poll until the second GET reflects the updated bucket count.
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(3);
    let updated_runs: u64 = loop {
        let resp: serde_json::Value = client
            .get(format!(
                "{}/api/cost/workflows/{}?days=30",
                base_url, wf_id_str
            ))
            .send()
            .await
            .expect("GET 2")
            .json()
            .await
            .unwrap();
        let buckets = resp["cost_summary"]["daily_buckets"]
            .as_array()
            .expect("daily_buckets");
        let runs = buckets
            .iter()
            .find(|b| b["date"].as_str() == Some(today.as_str()))
            .map(|b| b["runs_completed"].as_u64().unwrap_or(0))
            .unwrap_or(0);
        if runs >= 2 {
            break runs;
        }
        if tokio::time::Instant::now() >= deadline {
            break runs;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    };
    assert_eq!(
        updated_runs, 2,
        "after cache invalidation, bucket should reflect 2 completed runs"
    );
}

// ---------------------------------------------------------------------------
// v4.2.10 lean /api/workflows verification tests
// ---------------------------------------------------------------------------

/// v4.2.10-L1. GET /api/workflows returns a plain array with no cost_summary on each entry.
#[tokio::test]
async fn test_workflows_list_does_not_include_cost_summary() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("lean-list-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .error_for_status()
        .expect("201 created");

    let resp = client
        .get(format!("{}/api/workflows", base_url))
        .send()
        .await
        .expect("GET /api/workflows");
    assert_eq!(resp.status(), 200);
    let json: serde_json::Value = resp.json().await.unwrap();

    // Response must be a plain array (not wrapped).
    let arr = json
        .as_array()
        .expect("GET /api/workflows must return a plain array");
    assert_eq!(arr.len(), 1);

    // No workflow in the array must carry a cost_summary key.
    for wf in arr {
        assert!(
            wf.get("cost_summary").is_none() || wf["cost_summary"].is_null(),
            "workflow in list must not include cost_summary (moved to /api/cost/workflows)"
        );
    }
}

/// v4.2.10-L2. GET /api/workflows/{id} returns a lean Workflow with no cost_summary.
#[tokio::test]
async fn test_workflows_singular_does_not_include_cost_summary() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("lean-singular-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let json: serde_json::Value = client
        .get(format!("{}/api/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET /api/workflows/{id}")
        .json()
        .await
        .unwrap();

    assert_eq!(json["id"].as_str().unwrap(), id);
    assert!(
        json.get("cost_summary").is_none() || json["cost_summary"].is_null(),
        "GET /api/workflows/{{id}} must not include cost_summary (moved to /api/cost/workflows/{{id}})"
    );
}

/// v4.2.10-L3. GET /api/workflows?days=30 ignores cost query params and returns 200.
/// (The lean /api/workflows endpoint does not validate or use cost query params.)
#[tokio::test]
async fn test_workflows_list_ignores_cost_query_params() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) =
        spawn_test_server(wf_store, run_store, tmp.path().to_path_buf()).await;
    let client = reqwest::Client::new();

    // The lean endpoint should return 200 and a plain array regardless of cost params.
    let resp = client
        .get(format!("{}/api/workflows?days=30", base_url))
        .send()
        .await
        .expect("GET /api/workflows?days=30");
    assert_eq!(
        resp.status(),
        200,
        "GET /api/workflows?days=30 should return 200 (params ignored by lean endpoint)"
    );
    let json: serde_json::Value = resp.json().await.unwrap();
    assert!(
        json.is_array(),
        "response must still be a plain array when cost query params are passed to lean endpoint"
    );
}

/// A killed step must record `log_byte_offset_end` on its `StepRun`. The step
/// impls write the END marker to the log file even on the kill path (before
/// surfacing the `StepError::Killed`), so the executor can capture that
/// offset and persist it onto the `StepRun.log_byte_offset_end` field.
///
/// We trigger a long-sleep workflow, POST `/kill`, wait for the run to
/// transition to `Killed`, then assert:
///   1. `log_byte_offset_start` is `> 0` (the START marker landed)
///   2. `log_byte_offset_end` is `Some(N)` with `N > log_byte_offset_start`
#[tokio::test]
async fn test_killed_step_records_log_byte_offset_end() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let tmp_path = tmp.path().to_path_buf();
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp_path.clone(),
    )
    .await;
    let client = reqwest::Client::new();

    #[cfg(windows)]
    let sleep_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
    #[cfg(not(windows))]
    let sleep_cmd = "sleep 30";

    let slow_step = StepDef::Shell(ShellStep {
        common: StepDefCommon {
            id: "slow".to_string(),
            on_failure: None,
            always_run: false,
            timeout_secs: Some(60),
            working_dir: None,
            env_vars: None,
            capture: CaptureSpec::default(),
        },
        command: sleep_cmd.to_string(),
        pass_stdin: false,
    });
    let wf = NewWorkflow {
        name: "kill-end-offset-capture".to_string(),
        schedule: "*/5 * * * *".to_string(),
        timezone: None,
        schedule_mode: Default::default(),
        enabled: true,
        steps: vec![slow_step],
        default_input: None,
        working_dir: None,
        env_vars: None,
        allow_concurrent: None,
        on_failure: FailurePolicy::default(),
    };

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&wf).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let trigger_json: serde_json::Value = client
        .post(format!("{}/api/workflows/{}/trigger", base_url, wf_id))
        .header("Content-Type", "application/json")
        .body(r#"{"input": null}"#)
        .send()
        .await
        .expect("trigger")
        .json()
        .await
        .unwrap();
    let run_id = trigger_json["run_id"].as_str().unwrap();

    // Give the executor a moment to start the step (write START marker).
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    // Send the kill.
    let kill_resp = client
        .post(format!("{}/api/runs/{}/kill", base_url, run_id))
        .send()
        .await
        .expect("POST /kill");
    assert_eq!(kill_resp.status(), 202, "kill endpoint should return 202");

    // Poll until terminal.
    let deadline = tokio::time::Instant::now() + tokio::time::Duration::from_secs(10);
    let final_run = loop {
        if tokio::time::Instant::now() >= deadline {
            panic!("run {} did not finish within 10s after kill", run_id);
        }
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;
        let run_resp = client
            .get(format!("{}/api/runs/{}", base_url, run_id))
            .send()
            .await
            .expect("GET run");
        if run_resp.status() == 200 {
            let run_json: serde_json::Value = run_resp.json().await.unwrap();
            let status = run_json["status"].as_str().unwrap_or("");
            if matches!(status, "Killed" | "Failed" | "Completed") {
                break run_json;
            }
        }
    };

    assert_eq!(
        final_run["status"].as_str().unwrap_or(""),
        "Killed",
        "run should finish with Killed status after kill signal"
    );

    let steps = final_run["steps"].as_array().expect("steps array");
    let slow = steps
        .iter()
        .find(|s| s["step_id"] == "slow")
        .expect("slow step recorded");

    let start = slow["log_byte_offset_start"]
        .as_u64()
        .expect("log_byte_offset_start present");
    // The exact value of `start` is not asserted — depends on log header
    // bytes written before any step. The invariant we care about is that
    // `log_byte_offset_end` is Some and strictly exceeds it.
    let end = slow["log_byte_offset_end"]
        .as_u64()
        .expect("log_byte_offset_end must be Some(N) for a killed step that wrote the END marker on its kill-path");
    assert!(
        end > start,
        "end offset ({}) must be strictly greater than start offset ({})",
        end,
        start
    );
}

// ---------------------------------------------------------------------------
// Token tracking on cost endpoints — v4.2.11
// ---------------------------------------------------------------------------
//
// These tests assert that the cost API surfaces input/output token counts
// alongside USD costs. Tokens come from ClaudeStreamParser (summing
// `usage.iterations[]` across all agent steps in a run) and are persisted
// onto WorkflowRun.total_input_tokens / total_output_tokens at run completion.
//
// CostSummary exposes rolling-window totals (last_30_days / last_year
// × input / output) and each DailyBucket exposes per-status totals
// (tokens_in_from_completed / _failed / _killed, tokens_out_from_*) plus
// cross-status sums (total_input_tokens / total_output_tokens).

/// TOK-1. GET /api/cost/workflows/{id} surfaces the four new CostSummary
/// token fields, defaulting to 0 when no agent runs exist.
#[tokio::test]
async fn test_cost_summary_includes_token_fields_zero_default() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("tok1-empty-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let id = created["id"].as_str().unwrap();

    let json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, id))
        .send()
        .await
        .expect("GET /api/cost/workflows/{id}")
        .json()
        .await
        .unwrap();

    let cs = &json["cost_summary"];
    assert!(!cs.is_null(), "cost_summary must be present");

    // All four new token fields must be present and default to 0.
    for field in [
        "last_30_days_input_tokens",
        "last_30_days_output_tokens",
        "last_year_input_tokens",
        "last_year_output_tokens",
    ] {
        let v = &cs[field];
        assert!(
            v.is_number(),
            "cost_summary.{} must be a number, got {:?}",
            field,
            v
        );
        assert_eq!(
            v.as_u64().unwrap(),
            0,
            "cost_summary.{} must default to 0 when no runs exist",
            field
        );
    }
}

/// TOK-2. GET /api/cost/workflows/{id} aggregates token totals across
/// terminal runs and surfaces them on CostSummary (rolling-window)
/// AND on each DailyBucket (per-status + cross-status totals).
#[tokio::test]
async fn test_cost_summary_aggregates_token_totals() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("tok2-agg-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    // Use yesterday (UTC-1d) so finished_at lands safely inside the
    // 30-day window regardless of the test runner's local timezone.
    let yesterday = Utc::now() - Duration::days(1);

    // 2 Completed runs: 1000 in / 500 out and 2000 in / 1000 out.
    insert_terminal_run_with_tokens(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        yesterday,
        Some(0.10),
        1000,
        500,
    )
    .await;
    insert_terminal_run_with_tokens(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        yesterday,
        Some(0.20),
        2000,
        1000,
    )
    .await;
    // 1 Failed run: 300 in / 100 out.
    insert_terminal_run_with_tokens(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Failed,
        yesterday,
        Some(0.05),
        300,
        100,
    )
    .await;
    // 1 Killed run: 50 in / 25 out.
    insert_terminal_run_with_tokens(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Killed,
        yesterday,
        None,
        50,
        25,
    )
    .await;

    let json: serde_json::Value = client
        .get(format!(
            "{}/api/cost/workflows/{}?days=30",
            base_url, wf_id_str
        ))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();

    let cs = &json["cost_summary"];

    // Rolling-window totals: completed (3000/1500) + failed (300/100) + killed (50/25).
    let expected_in = 3000u64 + 300 + 50;
    let expected_out = 1500u64 + 100 + 25;
    assert_eq!(
        cs["last_30_days_input_tokens"].as_u64().unwrap(),
        expected_in,
        "last_30_days_input_tokens should sum across all terminal statuses"
    );
    assert_eq!(
        cs["last_30_days_output_tokens"].as_u64().unwrap(),
        expected_out,
        "last_30_days_output_tokens should sum across all terminal statuses"
    );
    // 1-year window is a superset of 30 days, so for this fixture it
    // must contain at least the same counts (runs from yesterday).
    assert!(
        cs["last_year_input_tokens"].as_u64().unwrap() >= expected_in,
        "last_year_input_tokens should be >= last_30_days_input_tokens"
    );
    assert!(
        cs["last_year_output_tokens"].as_u64().unwrap() >= expected_out,
        "last_year_output_tokens should be >= last_30_days_output_tokens"
    );

    // DailyBucket: locate yesterday's bucket and verify per-status splits +
    // cross-status totals are present and add up.
    let buckets = cs["daily_buckets"]
        .as_array()
        .expect("daily_buckets is array");
    assert!(
        !buckets.is_empty(),
        "expected at least one bucket for yesterday's runs"
    );

    // Sum across all returned buckets — runs may be assigned to either
    // yesterday or today depending on display_tz offset, but the totals
    // must equal the input fixture.
    let mut sum_in_c = 0u64;
    let mut sum_in_f = 0u64;
    let mut sum_in_k = 0u64;
    let mut sum_out_c = 0u64;
    let mut sum_out_f = 0u64;
    let mut sum_out_k = 0u64;
    let mut sum_total_in = 0u64;
    let mut sum_total_out = 0u64;

    for b in buckets {
        // All eight new bucket fields must exist as numbers.
        for field in [
            "tokens_in_from_completed",
            "tokens_in_from_failed",
            "tokens_in_from_killed",
            "tokens_out_from_completed",
            "tokens_out_from_failed",
            "tokens_out_from_killed",
            "total_input_tokens",
            "total_output_tokens",
        ] {
            assert!(
                b[field].is_number(),
                "DailyBucket.{} must be a number, got {:?}",
                field,
                b[field]
            );
        }

        let in_c = b["tokens_in_from_completed"].as_u64().unwrap();
        let in_f = b["tokens_in_from_failed"].as_u64().unwrap();
        let in_k = b["tokens_in_from_killed"].as_u64().unwrap();
        let out_c = b["tokens_out_from_completed"].as_u64().unwrap();
        let out_f = b["tokens_out_from_failed"].as_u64().unwrap();
        let out_k = b["tokens_out_from_killed"].as_u64().unwrap();
        let tot_in = b["total_input_tokens"].as_u64().unwrap();
        let tot_out = b["total_output_tokens"].as_u64().unwrap();

        // Cross-status totals must equal the sum of the per-status fields.
        assert_eq!(
            tot_in,
            in_c + in_f + in_k,
            "DailyBucket.total_input_tokens must equal sum of tokens_in_from_{{completed,failed,killed}}"
        );
        assert_eq!(
            tot_out,
            out_c + out_f + out_k,
            "DailyBucket.total_output_tokens must equal sum of tokens_out_from_{{completed,failed,killed}}"
        );

        sum_in_c += in_c;
        sum_in_f += in_f;
        sum_in_k += in_k;
        sum_out_c += out_c;
        sum_out_f += out_f;
        sum_out_k += out_k;
        sum_total_in += tot_in;
        sum_total_out += tot_out;
    }

    assert_eq!(sum_in_c, 3000, "tokens_in_from_completed total");
    assert_eq!(sum_in_f, 300, "tokens_in_from_failed total");
    assert_eq!(sum_in_k, 50, "tokens_in_from_killed total");
    assert_eq!(sum_out_c, 1500, "tokens_out_from_completed total");
    assert_eq!(sum_out_f, 100, "tokens_out_from_failed total");
    assert_eq!(sum_out_k, 25, "tokens_out_from_killed total");
    assert_eq!(sum_total_in, expected_in);
    assert_eq!(sum_total_out, expected_out);
}

/// TOK-3. GET /api/cost/workflows (system aggregate) surfaces the four new
/// token fields on the wrapped `system_cost_summary` block, aggregating
/// across all workflows.
#[tokio::test]
async fn test_system_cost_summary_includes_token_fields() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Two workflows, each with one Completed run carrying tokens.
    let mut wf_token_pairs: Vec<(u64, u64)> = Vec::new();
    for (name, in_tokens, out_tokens) in [
        ("tok3-wf-a", 1000u64, 500u64),
        ("tok3-wf-b", 2000u64, 750u64),
    ] {
        let created: serde_json::Value = client
            .post(format!("{}/api/workflows", base_url))
            .header("Content-Type", "application/json")
            .body(serde_json::to_string(&make_new_workflow(name)).unwrap())
            .send()
            .await
            .expect("POST workflow")
            .json()
            .await
            .unwrap();
        let wf_id = uuid::Uuid::parse_str(created["id"].as_str().unwrap()).unwrap();
        let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

        insert_terminal_run_with_tokens(
            &run_store,
            wf_id,
            &wf,
            RunStatus::Completed,
            Utc::now() - Duration::days(1),
            Some(0.10),
            in_tokens,
            out_tokens,
        )
        .await;
        wf_token_pairs.push((in_tokens, out_tokens));
    }

    let json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET /api/cost/workflows list")
        .json()
        .await
        .unwrap();

    let scs = &json["system_cost_summary"];
    assert!(!scs.is_null(), "system_cost_summary must be present");

    // All four new token fields must exist on the system aggregate.
    for field in [
        "last_30_days_input_tokens",
        "last_30_days_output_tokens",
        "last_year_input_tokens",
        "last_year_output_tokens",
    ] {
        assert!(
            scs[field].is_number(),
            "system_cost_summary.{} must be a number",
            field
        );
    }

    let expected_in: u64 = wf_token_pairs.iter().map(|(i, _)| *i).sum();
    let expected_out: u64 = wf_token_pairs.iter().map(|(_, o)| *o).sum();
    assert_eq!(
        scs["last_30_days_input_tokens"].as_u64().unwrap(),
        expected_in
    );
    assert_eq!(
        scs["last_30_days_output_tokens"].as_u64().unwrap(),
        expected_out
    );
    assert_eq!(scs["last_year_input_tokens"].as_u64().unwrap(), expected_in);
    assert_eq!(
        scs["last_year_output_tokens"].as_u64().unwrap(),
        expected_out
    );
}

/// AVG-1. Empty workflow: avg-cost-per-run fields return 0.0 (not null, not NaN)
///        when there are no runs in the window.
#[tokio::test]
async fn test_cost_summary_avg_zero_when_no_runs() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("avg-empty-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id = created["id"].as_str().unwrap();

    let json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();

    let cs = &json["cost_summary"];
    // Both fields exist as numbers (not null) and equal 0.0 when runs == 0.
    assert_eq!(
        cs["last_30_days_avg_cost_per_run_usd"].as_f64().unwrap(),
        0.0
    );
    assert_eq!(cs["last_year_avg_cost_per_run_usd"].as_f64().unwrap(), 0.0);
}

/// AVG-2. With seeded terminal runs, avg-cost-per-run equals total / runs at
///        both the 30-day and 365-day windows, and on the system aggregate.
#[tokio::test]
async fn test_cost_summary_avg_equals_total_over_runs() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("avg-totals-wf")).unwrap())
        .send()
        .await
        .expect("POST workflow")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap();
    let wf_id = uuid::Uuid::parse_str(wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    let yesterday = Utc::now() - Duration::days(1);

    // 3 terminal runs across statuses with explicit costs:
    // Completed 0.10, Completed 0.30, Failed 0.20 → total = 0.60, runs = 3, avg = 0.20
    insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        yesterday,
        Some(0.10),
    )
    .await;
    insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        yesterday,
        Some(0.30),
    )
    .await;
    insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Failed,
        yesterday,
        Some(0.20),
    )
    .await;

    // Singular endpoint: per-workflow cost_summary.
    let json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET singular")
        .json()
        .await
        .unwrap();
    let cs = &json["cost_summary"];

    let total_30 = cs["last_30_days_total_usd"].as_f64().unwrap();
    let runs_30 = cs["last_30_days_runs"].as_u64().unwrap();
    let avg_30 = cs["last_30_days_avg_cost_per_run_usd"].as_f64().unwrap();
    assert!(runs_30 >= 3, "expected at least the 3 seeded runs in 30d");
    assert!(
        (avg_30 - total_30 / runs_30 as f64).abs() < 1e-9,
        "30d avg {} should equal total {} / runs {}",
        avg_30,
        total_30,
        runs_30
    );

    let total_y = cs["last_year_total_usd"].as_f64().unwrap();
    let runs_y = cs["last_year_runs"].as_u64().unwrap();
    let avg_y = cs["last_year_avg_cost_per_run_usd"].as_f64().unwrap();
    assert!(
        (avg_y - total_y / runs_y as f64).abs() < 1e-9,
        "year avg {} should equal total {} / runs {}",
        avg_y,
        total_y,
        runs_y
    );

    // System aggregate: same identity on system_cost_summary.
    let list_json: serde_json::Value = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET list")
        .json()
        .await
        .unwrap();
    let scs = &list_json["system_cost_summary"];
    let stotal_30 = scs["last_30_days_total_usd"].as_f64().unwrap();
    let sruns_30 = scs["last_30_days_runs"].as_u64().unwrap();
    let savg_30 = scs["last_30_days_avg_cost_per_run_usd"].as_f64().unwrap();
    assert!(sruns_30 >= 3);
    assert!(
        (savg_30 - stotal_30 / sruns_30 as f64).abs() < 1e-9,
        "system 30d avg {} should equal total {} / runs {}",
        savg_30,
        stotal_30,
        sruns_30
    );
}

// ---------------------------------------------------------------------------
// Soft-delete tests (ACS-25)
// ---------------------------------------------------------------------------

/// SD-1. Soft-deleting a workflow that has run history returns 204, hides the
///       workflow from GET/list, keeps the runs queryable, and keeps its cost
///       history visible (flagged `workflow_deleted: true`).
#[tokio::test]
async fn test_soft_delete_preserves_runs_and_cost() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("sd-preserve")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap().to_string();
    let wf_id = Uuid::parse_str(&wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    let run_id = insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(0.42),
    )
    .await;

    // DELETE → 204 even though run history exists (regression: used to 500 on
    // the FOREIGN KEY constraint).
    let del = client
        .delete(format!("{}/api/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(
        del.status(),
        204,
        "soft delete with run history must be 204"
    );

    // GET by id → 404; absent from list.
    assert_eq!(
        client
            .get(format!("{}/api/workflows/{}", base_url, wf_id_str))
            .send()
            .await
            .expect("GET")
            .status(),
        404
    );
    let list: serde_json::Value = client
        .get(format!("{}/api/workflows", base_url))
        .send()
        .await
        .expect("GET list")
        .json()
        .await
        .unwrap();
    assert!(
        list.as_array().unwrap().is_empty(),
        "deleted workflow must be absent from the list"
    );

    // The run is still queryable.
    let run_resp = client
        .get(format!("{}/api/runs/{}", base_url, run_id))
        .send()
        .await
        .expect("GET run");
    assert_eq!(
        run_resp.status(),
        200,
        "runs must survive workflow deletion"
    );

    // Per-id cost endpoint → 200 with workflow_deleted: true and the run's cost.
    let cost: serde_json::Value = client
        .get(format!("{}/api/cost/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("GET cost by id")
        .json()
        .await
        .unwrap();
    assert_eq!(cost["workflow_deleted"], true, "flag must be true");
    assert_eq!(cost["cost_summary"]["last_30_days_runs"], 1);

    // Cost list includes the deleted workflow, flagged.
    let cost_list: serde_json::Value = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET cost list")
        .json()
        .await
        .unwrap();
    let entry = cost_list["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["workflow_id"] == serde_json::json!(wf_id_str))
        .expect("deleted workflow present in cost list");
    assert_eq!(entry["workflow_deleted"], true);
}

/// SD-2. Deleting a workflow with an active (Running) run is rejected with 409
///       and the workflow is left intact.
#[tokio::test]
async fn test_delete_workflow_with_active_run_returns_409() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("sd-active")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap().to_string();
    let wf_id = Uuid::parse_str(&wf_id_str).unwrap();
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();

    insert_running_run(&run_store, wf_id, &wf).await;

    let del = client
        .delete(format!("{}/api/workflows/{}", base_url, wf_id_str))
        .send()
        .await
        .expect("DELETE");
    assert_eq!(del.status(), 409, "active run must block delete");
    let body: serde_json::Value = del.json().await.unwrap();
    assert_eq!(body["error"], "workflow_run_active");

    // Workflow still exists.
    assert_eq!(
        client
            .get(format!("{}/api/workflows/{}", base_url, wf_id_str))
            .send()
            .await
            .expect("GET")
            .status(),
        200,
        "workflow must remain after a blocked delete"
    );
}

/// SD-3. A name can be reused after soft-delete, and the cost list then shows
///       two separate entries (deleted + live) that key by distinct ids.
#[tokio::test]
async fn test_name_reuse_after_soft_delete_shows_two_cost_entries() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // Create → run → delete.
    let first: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("reuse-name")).unwrap())
        .send()
        .await
        .expect("POST 1")
        .json()
        .await
        .unwrap();
    let first_id = first["id"].as_str().unwrap().to_string();
    let first_uuid = Uuid::parse_str(&first_id).unwrap();
    let first_wf = wf_store.get_workflow(first_uuid).await.unwrap().unwrap();
    insert_terminal_run(
        &run_store,
        first_uuid,
        &first_wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(0.10),
    )
    .await;
    assert_eq!(
        client
            .delete(format!("{}/api/workflows/{}", base_url, first_id))
            .send()
            .await
            .expect("DELETE")
            .status(),
        204
    );

    // Recreate the same name — must succeed with a new id.
    let second: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("reuse-name")).unwrap())
        .send()
        .await
        .expect("POST 2")
        .json()
        .await
        .unwrap();
    assert_eq!(second.get("error"), None, "recreate must not conflict");
    let second_id = second["id"].as_str().unwrap().to_string();
    assert_ne!(second_id, first_id, "reused name gets a new id");
    let second_uuid = Uuid::parse_str(&second_id).unwrap();
    let second_wf = wf_store.get_workflow(second_uuid).await.unwrap().unwrap();
    insert_terminal_run(
        &run_store,
        second_uuid,
        &second_wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(0.20),
    )
    .await;

    // Cost list shows two separate entries with the same name.
    let cost_list: serde_json::Value = client
        .get(format!("{}/api/cost/workflows", base_url))
        .send()
        .await
        .expect("GET cost list")
        .json()
        .await
        .unwrap();
    let entries: Vec<&serde_json::Value> = cost_list["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["workflow_name"] == serde_json::json!("reuse-name"))
        .collect();
    assert_eq!(entries.len(), 2, "expected a deleted + a live entry");
    let deleted_entry = entries
        .iter()
        .find(|e| e["workflow_id"] == serde_json::json!(first_id))
        .expect("deleted entry");
    let live_entry = entries
        .iter()
        .find(|e| e["workflow_id"] == serde_json::json!(second_id))
        .expect("live entry");
    assert_eq!(deleted_entry["workflow_deleted"], true);
    assert_eq!(live_entry["workflow_deleted"], false);
}

/// SD-4. On soft-delete, the run-log directory and the ACS-managed default
///       operating folder are removed, and the run-log endpoint no longer 500s.
#[tokio::test]
async fn test_soft_delete_removes_default_working_dir_and_log_dir() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let data_dir = tmp.path().to_path_buf();
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        data_dir.clone(),
    )
    .await;
    let client = reqwest::Client::new();

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&make_new_workflow("sd-wd-default")).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap().to_string();
    let wf_id = Uuid::parse_str(&wf_id_str).unwrap();

    // The default working_dir was created on disk under the test's workflow_dirs.
    let working_dir = created["working_dir"].as_str().unwrap().to_string();
    let working_dir_path = std::path::PathBuf::from(&working_dir);
    assert!(
        working_dir_path.exists(),
        "default working_dir should exist after create"
    );

    // Fabricate a run-log directory as the trigger path would.
    let log_dir = data_dir.join("logs").join(&wf_id_str);
    std::fs::create_dir_all(&log_dir).expect("create log dir");
    std::fs::write(log_dir.join("some.log"), b"log data").expect("write log file");

    // Seed a terminal run so there is a run row (its log file is gone below).
    let wf = wf_store.get_workflow(wf_id).await.unwrap().unwrap();
    let run_id = insert_terminal_run(
        &run_store,
        wf_id,
        &wf,
        RunStatus::Completed,
        Utc::now() - Duration::days(1),
        Some(0.05),
    )
    .await;

    assert_eq!(
        client
            .delete(format!("{}/api/workflows/{}", base_url, wf_id_str))
            .send()
            .await
            .expect("DELETE")
            .status(),
        204
    );

    assert!(
        !working_dir_path.exists(),
        "ACS-managed default working_dir must be removed on delete"
    );
    assert!(
        !log_dir.exists(),
        "run-log directory must be removed on delete"
    );

    // The run-log endpoint must not 500 now that the file is gone (404 is ok).
    let log_resp = client
        .get(format!("{}/api/runs/{}/log", base_url, run_id))
        .send()
        .await
        .expect("GET run log");
    assert_eq!(
        log_resp.status(),
        404,
        "missing log after delete should be 404, never 500"
    );
}

/// SD-5. A custom (user-specified) working_dir is NEVER touched on delete.
#[tokio::test]
async fn test_soft_delete_preserves_custom_working_dir() {
    let (wf_store, run_store, tmp) = make_stores().await;
    let (base_url, _state, _handle) = spawn_test_server(
        Arc::clone(&wf_store),
        Arc::clone(&run_store),
        tmp.path().to_path_buf(),
    )
    .await;
    let client = reqwest::Client::new();

    // A custom directory that is NOT the ACS-managed default.
    let custom_dir = tmp.path().join("my-custom-dir");
    std::fs::create_dir_all(&custom_dir).expect("create custom dir");

    let mut body = make_new_workflow("sd-wd-custom");
    body.working_dir = Some(custom_dir.to_string_lossy().into_owned());

    let created: serde_json::Value = client
        .post(format!("{}/api/workflows", base_url))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(&body).unwrap())
        .send()
        .await
        .expect("POST")
        .json()
        .await
        .unwrap();
    let wf_id_str = created["id"].as_str().unwrap().to_string();

    assert_eq!(
        client
            .delete(format!("{}/api/workflows/{}", base_url, wf_id_str))
            .send()
            .await
            .expect("DELETE")
            .status(),
        204
    );

    assert!(
        custom_dir.exists(),
        "a custom working_dir must never be deleted on soft-delete"
    );
}
