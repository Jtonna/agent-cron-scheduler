//! Integration tests for the /api/workflows and /api/runs endpoints.
//!
//! Each test spawns a real Axum server on a random port and exercises the
//! workflow API end-to-end.

use std::sync::Arc;
use std::time::Instant;

use agent_cron_scheduler::daemon::events::WorkflowEvent;
use agent_cron_scheduler::models::workflow::{
    CaptureSpec, FailurePolicy, NewWorkflow, ShellStep, StepDef, StepDefCommon, WorkflowUpdate,
};
use agent_cron_scheduler::models::DaemonConfig;
use agent_cron_scheduler::server::{self, AppState};
use agent_cron_scheduler::storage::workflow_runs::WorkflowRunStore;
use agent_cron_scheduler::storage::workflows::WorkflowStore;

use chrono::Utc;
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

    let mut config = DaemonConfig::default();
    config.data_dir = Some(data_dir);

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
        input_schema: None,
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

/// 2. GET /api/workflows lists includes the created workflow
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
        input_schema: None,
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
        input_schema: None,
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
    use agent_cron_scheduler::models::workflow::{RunStatus, WorkflowRun};
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
        input_schema: None,
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
        input_schema: None,
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
        input_schema: None,
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
        input_schema: None,
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
