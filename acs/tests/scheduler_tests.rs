//! End-to-end scheduler tests.
//!
//! These tests verify that the full lifecycle of create -> schedule -> run
//! works correctly with real storage (using temp directories).

use std::sync::Arc;

use agent_cron_scheduler::daemon::events::JobEvent;
use agent_cron_scheduler::daemon::executor::Executor;
use agent_cron_scheduler::models::{DaemonConfig, ExecutionType, Job};
use agent_cron_scheduler::pty::MockPtySpawner;
use agent_cron_scheduler::storage::logs::FsLogStore;
use agent_cron_scheduler::storage::LogStore;

use chrono::Utc;
use tempfile::TempDir;
use tokio::sync::broadcast;
use uuid::Uuid;

fn make_job(name: &str) -> Job {
    let now = Utc::now();
    Job {
        id: Uuid::now_v7(),
        name: name.to_string(),
        schedule: "* * * * *".to_string(),
        execution: ExecutionType::ShellCommand("echo hello".to_string()),
        enabled: true,
        timezone: None,
        working_dir: None,
        env_vars: None,
        timeout_secs: 0,
        log_environment: false,
        created_at: now,
        updated_at: now,
        last_run_at: None,
        last_exit_code: None,
        next_run_at: None,
    }
}

#[tokio::test]
async fn test_end_to_end_create_run_verify_logs() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let data_dir = tmp_dir.path().to_path_buf();

    // Create a real FsLogStore
    let log_store = Arc::new(
        FsLogStore::new(data_dir.clone())
            .await
            .expect("create log store"),
    ) as Arc<dyn LogStore>;

    let config = Arc::new(DaemonConfig::default());
    let (event_tx, mut event_rx) = broadcast::channel::<JobEvent>(4096);

    // Use a mock PTY spawner that produces output
    let spawner = MockPtySpawner::with_output_and_exit(vec![b"hello world\n".to_vec()], 0);
    let pty_spawner = Arc::new(spawner) as Arc<dyn agent_cron_scheduler::pty::PtySpawner>;

    let executor = Executor::new(
        event_tx.clone(),
        Arc::clone(&log_store),
        config,
        pty_spawner,
    );

    // Create and run a job
    let job = make_job("e2e-test");
    let handle = executor.spawn_job(&job).await.expect("spawn_job");
    let run_id = handle.run_id;
    let job_id = handle.job_id;

    // Wait for the job to complete
    handle.join_handle.await.expect("join");

    // Give log writer time to flush
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Collect events
    let mut events = Vec::new();
    while let Ok(event) = event_rx.try_recv() {
        events.push(event);
    }

    // Verify we got Started, Output, and Completed events
    let started = events.iter().any(|e| matches!(e, JobEvent::Started { .. }));
    assert!(started, "Should have Started event");

    let completed = events
        .iter()
        .any(|e| matches!(e, JobEvent::Completed { .. }));
    assert!(completed, "Should have Completed event");

    // Verify log content was written to disk
    let log_content = log_store
        .read_log(job_id, run_id, None)
        .await
        .expect("read log");
    assert!(
        log_content.contains("hello world"),
        "Log should contain 'hello world', got: {}",
        log_content
    );

    // Verify runs are listed
    let (runs, total) = log_store.list_runs(job_id, 10, 0).await.expect("list runs");
    assert_eq!(total, 1);
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].run_id, run_id);
}

#[tokio::test]
async fn test_end_to_end_log_cleanup_works_with_real_store() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let data_dir = tmp_dir.path().to_path_buf();

    let log_store = Arc::new(
        FsLogStore::new(data_dir.clone())
            .await
            .expect("create log store"),
    ) as Arc<dyn LogStore>;

    // Use a config with max_log_files_per_job = 2
    let config = DaemonConfig {
        max_log_files_per_job: 2,
        ..Default::default()
    };
    let config = Arc::new(config);

    let (event_tx, _event_rx) = broadcast::channel::<JobEvent>(4096);
    let spawner = MockPtySpawner::with_output_and_exit(vec![b"run output\n".to_vec()], 0);
    let pty_spawner = Arc::new(spawner) as Arc<dyn agent_cron_scheduler::pty::PtySpawner>;

    let _executor = Executor::new(
        event_tx.clone(),
        Arc::clone(&log_store),
        Arc::clone(&config),
        pty_spawner,
    );

    let job = make_job("cleanup-test");
    let job_id = job.id;

    // Run the job 4 times
    for _ in 0..4 {
        let spawner2 = MockPtySpawner::with_output_and_exit(vec![b"run\n".to_vec()], 0);
        let pty_spawner2 = Arc::new(spawner2) as Arc<dyn agent_cron_scheduler::pty::PtySpawner>;
        let executor2 = Executor::new(
            event_tx.clone(),
            Arc::clone(&log_store),
            Arc::clone(&config),
            pty_spawner2,
        );

        let handle = executor2.spawn_job(&job).await.expect("spawn");
        handle.join_handle.await.expect("join");
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Wait for cleanup to finish
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Should only have 2 runs left (max_log_files_per_job = 2)
    let (_runs, total) = log_store.list_runs(job_id, 100, 0).await.expect("list");
    assert_eq!(
        total, 2,
        "Should have 2 runs after cleanup (max_log_files=2), got {}",
        total
    );
}

#[tokio::test]
async fn test_end_to_end_failed_run_records_error() {
    let tmp_dir = TempDir::new().expect("create temp dir");
    let data_dir = tmp_dir.path().to_path_buf();

    let log_store = Arc::new(
        FsLogStore::new(data_dir.clone())
            .await
            .expect("create log store"),
    ) as Arc<dyn LogStore>;

    let config = Arc::new(DaemonConfig::default());
    let (event_tx, _event_rx) = broadcast::channel::<JobEvent>(4096);

    // Use a mock that fails to spawn
    let spawner = MockPtySpawner::with_spawn_error("PTY not available");
    let pty_spawner = Arc::new(spawner) as Arc<dyn agent_cron_scheduler::pty::PtySpawner>;

    let executor = Executor::new(event_tx, Arc::clone(&log_store), config, pty_spawner);

    let job = make_job("fail-test");
    let handle = executor.spawn_job(&job).await.expect("spawn");
    let run_id = handle.run_id;
    let job_id = handle.job_id;

    handle.join_handle.await.expect("join");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Verify the run was recorded as Failed
    let (runs, _) = log_store.list_runs(job_id, 10, 0).await.expect("list");
    assert_eq!(runs.len(), 1);

    let run = &runs[0];
    assert_eq!(run.run_id, run_id);
    assert_eq!(run.status, agent_cron_scheduler::models::RunStatus::Failed);
    assert!(run.error.is_some());
    assert!(run.error.as_ref().unwrap().contains("PTY not available"));
}
