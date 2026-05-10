//! SQLite implementation of [`WorkflowRunStore`].

use async_trait::async_trait;
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::{RunStatus, StepRun, Workflow, WorkflowRun};
use crate::storage::sqlite::row_helpers::{
    map_serde_err, map_uuid_err, parse_dt, parse_opt_dt, run_status_str,
};
use crate::storage::sqlite::SqliteDb;
use crate::storage::workflow_runs::WorkflowRunStore;

/// SQLite-backed implementation of [`WorkflowRunStore`].
pub struct SqliteWorkflowRunStore {
    db: SqliteDb,
}

impl SqliteWorkflowRunStore {
    /// Construct from a shared [`SqliteDb`]. The schema must already have been
    /// applied (call [`crate::storage::sqlite::init_db`] first).
    pub fn new(db: &SqliteDb) -> Self {
        Self { db: db.clone() }
    }

    /// Build an isolated in-memory store for unit tests.
    #[cfg(test)]
    fn in_memory_for_tests() -> Self {
        let db = crate::storage::sqlite::init_in_memory_db().expect("init in-memory db");
        Self::new(&db)
    }

    /// Build a store sharing a DB with a `SqliteWorkflowStore` for tests
    /// where the FK to `workflows.id` must resolve.
    #[cfg(test)]
    fn paired_for_tests() -> (
        crate::storage::sqlite::SqliteWorkflowStore,
        Self,
        crate::storage::sqlite::SqliteDb,
    ) {
        let db = crate::storage::sqlite::init_in_memory_db().expect("init in-memory db");
        let wf_store = crate::storage::sqlite::SqliteWorkflowStore::new(&db);
        let run_store = Self::new(&db);
        (wf_store, run_store, db)
    }
}

// ─── Row mapping ──────────────────────────────────────────────────────────────

fn row_to_run(row: &Row<'_>) -> rusqlite::Result<WorkflowRun> {
    let run_id_s: String = row.get("run_id")?;
    let run_id = Uuid::parse_str(&run_id_s).map_err(map_uuid_err)?;
    let workflow_id_s: String = row.get("workflow_id")?;
    let workflow_id = Uuid::parse_str(&workflow_id_s).map_err(map_uuid_err)?;

    let snapshot_json: String = row.get("workflow_snapshot")?;
    let workflow_snapshot: Workflow =
        serde_json::from_str(&snapshot_json).map_err(map_serde_err)?;

    let steps_json: String = row.get("steps_json")?;
    let steps: Vec<StepRun> = serde_json::from_str(&steps_json).map_err(map_serde_err)?;

    let status_s: String = row.get("status")?;
    let status: RunStatus =
        serde_json::from_value(serde_json::Value::String(status_s)).map_err(map_serde_err)?;

    let trigger_input_s: Option<String> = row.get("trigger_input")?;
    let trigger_input = match trigger_input_s {
        Some(s) => Some(serde_json::from_str::<serde_json::Value>(&s).map_err(map_serde_err)?),
        None => None,
    };

    Ok(WorkflowRun {
        run_id,
        workflow_id,
        workflow_version: row.get::<_, i64>("workflow_version")? as u32,
        workflow_snapshot,
        started_at: parse_dt(row, "started_at")?,
        finished_at: parse_opt_dt(row, "finished_at")?,
        status,
        trigger_input,
        steps,
        total_cost_usd: row.get("total_cost_usd")?,
        total_duration_ms: row
            .get::<_, Option<i64>>("total_duration_ms")?
            .map(|n| n as u64),
    })
}

fn upsert_run(conn: &Connection, run: &WorkflowRun) -> Result<(), AcsError> {
    let snapshot_json = serde_json::to_string(&run.workflow_snapshot)
        .map_err(|e| AcsError::Storage(e.to_string()))?;
    let steps_json =
        serde_json::to_string(&run.steps).map_err(|e| AcsError::Storage(e.to_string()))?;
    let trigger_input = run
        .trigger_input
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AcsError::Storage(e.to_string()))?;

    // INSERT OR REPLACE keeps both `create_run` and `update_run` simple. The
    // `Fs` store uses an atomic write-rename for both, with no logical
    // distinction between insert and update beyond an index check; mirror
    // that behaviour here. An UPSERT semantically matches.
    conn.execute(
        "INSERT INTO workflow_runs (
            run_id, workflow_id, workflow_version, workflow_snapshot,
            started_at, finished_at, status, trigger_input, steps_json,
            total_cost_usd, total_duration_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
         ON CONFLICT(run_id) DO UPDATE SET
            workflow_id       = excluded.workflow_id,
            workflow_version  = excluded.workflow_version,
            workflow_snapshot = excluded.workflow_snapshot,
            started_at        = excluded.started_at,
            finished_at       = excluded.finished_at,
            status            = excluded.status,
            trigger_input     = excluded.trigger_input,
            steps_json        = excluded.steps_json,
            total_cost_usd    = excluded.total_cost_usd,
            total_duration_ms = excluded.total_duration_ms",
        params![
            run.run_id.to_string(),
            run.workflow_id.to_string(),
            run.workflow_version as i64,
            snapshot_json,
            run.started_at.to_rfc3339(),
            run.finished_at.map(|d| d.to_rfc3339()),
            run_status_str(&run.status)?,
            trigger_input,
            steps_json,
            run.total_cost_usd,
            run.total_duration_ms.map(|n| n as i64),
        ],
    )
    .map_err(|e| AcsError::Storage(format!("INSERT/UPDATE workflow_run failed: {}", e)))?;
    Ok(())
}

// ─── Trait impl ───────────────────────────────────────────────────────────────

#[async_trait]
impl WorkflowRunStore for SqliteWorkflowRunStore {
    async fn create_run(&self, run: WorkflowRun) -> Result<(), AcsError> {
        self.db.with_conn(move |c| upsert_run(c, &run)).await
    }

    async fn update_run(&self, run: &WorkflowRun) -> Result<(), AcsError> {
        // Return NotFound if the run isn't already present — a presence
        // query is enough since the primary key already enforces uniqueness.
        let run_owned = run.clone();
        self.db
            .with_conn(move |c| {
                let exists: bool = c
                    .query_row(
                        "SELECT 1 FROM workflow_runs WHERE run_id = ?",
                        [run_owned.run_id.to_string()],
                        |_| Ok(true),
                    )
                    .optional()
                    .map_err(|e| AcsError::Storage(e.to_string()))?
                    .unwrap_or(false);
                if !exists {
                    return Err(AcsError::NotFound(format!(
                        "Run '{}' not found in index",
                        run_owned.run_id
                    )));
                }
                upsert_run(c, &run_owned)
            })
            .await
    }

    async fn get_run(&self, run_id: Uuid) -> Result<Option<WorkflowRun>, AcsError> {
        let id_s = run_id.to_string();
        self.db
            .with_conn(move |c| {
                let mut stmt = c
                    .prepare("SELECT * FROM workflow_runs WHERE run_id = ?")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                stmt.query_row([&id_s], row_to_run)
                    .optional()
                    .map_err(|e| AcsError::Storage(e.to_string()))
            })
            .await
    }

    async fn list_runs(
        &self,
        workflow_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowRun>, AcsError> {
        let wf_s = workflow_id.to_string();
        self.db
            .with_conn(move |c| {
                // Sort by run_id DESC. Since run_ids are Uuid v7 (time-ordered),
                // the lexicographic ordering matches insertion-time ordering.
                //
                // limit=0 means "no limit"; use SQLite's convention of -1.
                let sql_limit: i64 = if limit == 0 { -1 } else { limit as i64 };
                let sql_offset: i64 = offset as i64;
                let mut stmt = c
                    .prepare(
                        "SELECT * FROM workflow_runs \
                         WHERE workflow_id = ? \
                         ORDER BY run_id DESC \
                         LIMIT ? OFFSET ?",
                    )
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map(params![wf_s, sql_limit, sql_offset], row_to_run)
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| AcsError::Storage(e.to_string()))?);
                }
                Ok(out)
            })
            .await
    }

    async fn count_runs(&self, workflow_id: Uuid) -> Result<usize, AcsError> {
        let wf_s = workflow_id.to_string();
        self.db
            .with_conn(move |c| {
                let n: i64 = c
                    .query_row(
                        "SELECT COUNT(*) FROM workflow_runs WHERE workflow_id = ?",
                        [wf_s],
                        |r| r.get(0),
                    )
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                Ok(n as usize)
            })
            .await
    }

    async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError> {
        // Best-effort: deleting a row that isn't present is not an error.
        let id_s = run_id.to_string();
        self.db
            .with_conn(move |c| {
                c.execute("DELETE FROM workflow_runs WHERE run_id = ?", [id_s])
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                Ok(())
            })
            .await
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, NewWorkflow, ScheduleMode, ShellStep, StepDef, StepDefCommon,
        StepRun,
    };
    use crate::storage::sqlite::SqliteWorkflowStore;
    use crate::storage::workflows::WorkflowStore;
    use chrono::Utc;

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

    /// Create a workflow row via the paired wf_store and return the resulting
    /// `Workflow`. The run-store tests need a real parent row because
    /// `workflow_runs.workflow_id` has a FK on `workflows.id`.
    async fn seed_workflow(wf_store: &SqliteWorkflowStore, name: &str) -> Workflow {
        wf_store
            .create_workflow(NewWorkflow {
                name: name.to_string(),
                schedule: "*/5 * * * *".to_string(),
                timezone: None,
                schedule_mode: ScheduleMode::default(),
                enabled: true,
                steps: vec![make_shell_step("step-1")],
                input_schema: None,
                default_input: None,
                working_dir: None,
                env_vars: None,
                allow_concurrent: None,
                on_failure: FailurePolicy::default(),
            })
            .await
            .expect("seed workflow")
    }

    fn make_run(parent: &Workflow) -> WorkflowRun {
        WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: parent.id,
            workflow_version: parent.version,
            workflow_snapshot: parent.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
        }
    }

    fn make_completed_run(parent: &Workflow) -> WorkflowRun {
        let now = Utc::now();
        WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: parent.id,
            workflow_version: parent.version,
            workflow_snapshot: parent.clone(),
            started_at: now,
            finished_at: Some(now),
            status: RunStatus::Completed,
            trigger_input: Some(serde_json::json!({"k": "v"})),
            steps: vec![StepRun {
                step_index: 0,
                step_id: "step-1".to_string(),
                kind: "shell".to_string(),
                status: RunStatus::Completed,
                started_at: now,
                finished_at: Some(now),
                exit_code: Some(0),
                log_byte_offset_start: 0,
                log_byte_offset_end: Some(1024),
                cost_usd: Some(0.001),
                error: None,
                output_summary: Some(serde_json::json!({"summary": "ok"})),
            }],
            total_cost_usd: Some(0.001),
            total_duration_ms: Some(500),
        }
    }

    // ── Round-trip ────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_then_get_round_trip_minimal() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "p1").await;
        let run = make_run(&parent);
        let run_id = run.run_id;
        run_store.create_run(run.clone()).await.expect("create");
        let got = run_store
            .get_run(run_id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(run, got);
    }

    #[tokio::test]
    async fn test_create_then_get_round_trip_populated() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "p2").await;
        let run = make_completed_run(&parent);
        let run_id = run.run_id;
        run_store.create_run(run.clone()).await.expect("create");
        let got = run_store
            .get_run(run_id)
            .await
            .expect("get")
            .expect("present");
        // step array round-trips with same byte content
        assert_eq!(run.steps, got.steps);
        assert_eq!(run, got);
    }

    #[tokio::test]
    async fn test_get_run_missing_returns_none() {
        // No FK involved in a SELECT — in_memory_for_tests is fine here.
        let store = SqliteWorkflowRunStore::in_memory_for_tests();
        let got = store.get_run(Uuid::now_v7()).await.expect("get");
        assert!(got.is_none());
    }

    // ── Update ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_run_replaces_state() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "upd").await;
        let mut run = make_run(&parent);
        let run_id = run.run_id;
        run_store.create_run(run.clone()).await.expect("create");

        run.status = RunStatus::Completed;
        run.finished_at = Some(Utc::now());
        run.total_duration_ms = Some(123);
        run_store.update_run(&run).await.expect("update");

        let got = run_store
            .get_run(run_id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(got.status, RunStatus::Completed);
        assert!(got.finished_at.is_some());
        assert_eq!(got.total_duration_ms, Some(123));
    }

    #[tokio::test]
    async fn test_update_run_not_found() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "missing-update").await;
        let run = make_run(&parent);
        // Run was never created — update must fail with NotFound.
        let err = run_store.update_run(&run).await.expect_err("must error");
        assert!(matches!(err, AcsError::NotFound(_)), "got: {:?}", err);
    }

    // ── List / pagination ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_runs_latest_first() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "list-lf").await;
        let mut ids = vec![];
        for _ in 0..3 {
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
            let run = make_run(&parent);
            ids.push(run.run_id);
            run_store.create_run(run).await.expect("create");
        }
        let listed = run_store.list_runs(parent.id, 0, 0).await.expect("list");
        assert_eq!(listed.len(), 3);
        // Latest-first: the last inserted run id is first.
        assert_eq!(listed[0].run_id, ids[2]);
        assert_eq!(listed[2].run_id, ids[0]);
    }

    #[tokio::test]
    async fn test_list_runs_pagination_offset_limit() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "list-pag").await;
        let mut ids = vec![];
        for _ in 0..5 {
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
            let run = make_run(&parent);
            ids.push(run.run_id);
            run_store.create_run(run).await.expect("create");
        }
        // Latest-first: [4, 3, 2, 1, 0]; offset=2, limit=2 → [2, 1].
        let listed = run_store.list_runs(parent.id, 2, 2).await.expect("list");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].run_id, ids[2]);
        assert_eq!(listed[1].run_id, ids[1]);
    }

    #[tokio::test]
    async fn test_list_runs_limit_zero_returns_all_with_offset() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "list-zero").await;
        for _ in 0..4 {
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
            run_store
                .create_run(make_run(&parent))
                .await
                .expect("create");
        }
        let listed = run_store.list_runs(parent.id, 0, 1).await.expect("list");
        // limit=0 must mean "no limit"; offset=1 skips the newest.
        assert_eq!(listed.len(), 3);
    }

    #[tokio::test]
    async fn test_list_runs_filters_by_workflow_id() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf_a = seed_workflow(&wf_store, "filter-a").await;
        let wf_b = seed_workflow(&wf_store, "filter-b").await;
        for _ in 0..3 {
            run_store.create_run(make_run(&wf_a)).await.expect("a");
        }
        for _ in 0..1 {
            run_store.create_run(make_run(&wf_b)).await.expect("b");
        }
        assert_eq!(
            run_store
                .list_runs(wf_a.id, 0, 0)
                .await
                .expect("list a")
                .len(),
            3
        );
        assert_eq!(
            run_store
                .list_runs(wf_b.id, 0, 0)
                .await
                .expect("list b")
                .len(),
            1
        );
    }

    // ── Count ─────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_count_runs() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf_a = seed_workflow(&wf_store, "count-a").await;
        let wf_b = seed_workflow(&wf_store, "count-b").await;
        for _ in 0..5 {
            run_store.create_run(make_run(&wf_a)).await.expect("a");
        }
        for _ in 0..2 {
            run_store.create_run(make_run(&wf_b)).await.expect("b");
        }
        assert_eq!(run_store.count_runs(wf_a.id).await.expect("count a"), 5);
        assert_eq!(run_store.count_runs(wf_b.id).await.expect("count b"), 2);
        assert_eq!(run_store.count_runs(Uuid::now_v7()).await.expect("none"), 0);
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_run_removes_row() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let parent = seed_workflow(&wf_store, "del").await;
        let run = make_run(&parent);
        let id = run.run_id;
        run_store.create_run(run).await.expect("create");
        assert!(run_store.get_run(id).await.expect("get").is_some());
        run_store.delete_run(id).await.expect("delete");
        assert!(run_store.get_run(id).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn test_delete_run_unknown_is_no_op() {
        // Deleting a run that isn't present is a no-op.
        let store = SqliteWorkflowRunStore::in_memory_for_tests();
        store
            .delete_run(Uuid::now_v7())
            .await
            .expect("must not error on missing run");
    }
}
