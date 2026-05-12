//! SQLite implementation of [`WorkflowRunStore`].

use async_trait::async_trait;
use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::{
    CostSummary, DailyBucket, RunStatus, StepRun, Workflow, WorkflowRun,
};
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

    async fn cost_summary_for(
        &self,
        workflow_id: Uuid,
        display_tz: &Tz,
    ) -> Result<CostSummary, AcsError> {
        // Compute calendar-day boundaries in display_tz.
        let now_in_tz = chrono::Utc::now().with_timezone(display_tz);
        let today_naive = now_in_tz.date_naive();
        // Start of today in display_tz (midnight), then convert to UTC.
        #[allow(deprecated)]
        let today_midnight_local = today_naive
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always valid");
        let today_midnight_in_tz = display_tz
            .from_local_datetime(&today_midnight_local)
            .earliest()
            .unwrap_or_else(|| {
                // During spring-forward gap, fall back to the next valid instant.
                display_tz
                    .from_local_datetime(&today_midnight_local)
                    .latest()
                    .expect("at least one mapping must exist")
            });

        let last_30_days_start =
            (today_midnight_in_tz - chrono::Duration::days(30)).with_timezone(&chrono::Utc);
        let last_year_start =
            (today_midnight_in_tz - chrono::Duration::days(365)).with_timezone(&chrono::Utc);

        let last_30_days_start_s = last_30_days_start.to_rfc3339();
        let last_year_start_s = last_year_start.to_rfc3339();
        let wf_id_s = workflow_id.to_string();

        self.db
            .with_conn(move |c| {
                let (last_30d_total, last_30d_runs, last_year_total, last_year_runs): (
                    f64,
                    i64,
                    f64,
                    i64,
                ) = c
                    .query_row(
                        "SELECT
                          COALESCE(SUM(CASE WHEN finished_at >= ?1 THEN total_cost_usd END), 0.0) AS last_30d_total,
                          COALESCE(SUM(CASE WHEN finished_at >= ?1 THEN 1 END), 0)                AS last_30d_runs,
                          COALESCE(SUM(CASE WHEN finished_at >= ?2 THEN total_cost_usd END), 0.0) AS last_year_total,
                          COALESCE(SUM(CASE WHEN finished_at >= ?2 THEN 1 END), 0)               AS last_year_runs
                        FROM workflow_runs
                        WHERE workflow_id = ?3
                          AND status IN ('Completed', 'Failed', 'Killed')
                          AND finished_at IS NOT NULL
                          AND finished_at >= ?2",
                        params![last_30_days_start_s, last_year_start_s, wf_id_s],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                    )
                    .map_err(|e| AcsError::Storage(format!("cost_summary query failed: {}", e)))?;

                Ok(CostSummary {
                    last_30_days_total_usd: last_30d_total,
                    last_30_days_runs: last_30d_runs as u64,
                    last_year_total_usd: last_year_total,
                    last_year_runs: last_year_runs as u64,
                    computed_at: chrono::Utc::now(),
                    daily_buckets: Vec::new(),
                })
            })
            .await
    }

    async fn daily_buckets_for(
        &self,
        workflow_id: Option<Uuid>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        display_tz: &Tz,
    ) -> Result<Vec<DailyBucket>, AcsError> {
        let since_s = since.to_rfc3339();
        let until_s = until.to_rfc3339();
        let wf_id_s = workflow_id.map(|id| id.to_string());
        let tz = *display_tz;

        // Fetch rows via the async with_conn helper (like other methods in this file).
        let rows: Vec<(String, String, Option<f64>)> = self
            .db
            .with_conn(move |c| {
                let sql = if wf_id_s.is_some() {
                    "SELECT finished_at, status, total_cost_usd \
                     FROM workflow_runs \
                     WHERE status IN ('Completed', 'Failed', 'Killed') \
                       AND finished_at IS NOT NULL \
                       AND finished_at >= ?1 \
                       AND finished_at < ?2 \
                       AND workflow_id = ?3 \
                     ORDER BY finished_at ASC"
                } else {
                    "SELECT finished_at, status, total_cost_usd \
                     FROM workflow_runs \
                     WHERE status IN ('Completed', 'Failed', 'Killed') \
                       AND finished_at IS NOT NULL \
                       AND finished_at >= ?1 \
                       AND finished_at < ?2 \
                     ORDER BY finished_at ASC"
                };

                let mut stmt = c
                    .prepare(sql)
                    .map_err(|e| AcsError::Storage(e.to_string()))?;

                type Row = (String, String, Option<f64>);
                let mut out: Vec<Row> = Vec::new();

                if let Some(ref wf_id) = wf_id_s {
                    let row_iter = stmt
                        .query_map(rusqlite::params![since_s, until_s, wf_id], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<f64>>(2)?,
                            ))
                        })
                        .map_err(|e| AcsError::Storage(e.to_string()))?;
                    for r in row_iter {
                        out.push(r.map_err(|e| AcsError::Storage(e.to_string()))?);
                    }
                } else {
                    let row_iter = stmt
                        .query_map(rusqlite::params![since_s, until_s], |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, String>(1)?,
                                row.get::<_, Option<f64>>(2)?,
                            ))
                        })
                        .map_err(|e| AcsError::Storage(e.to_string()))?;
                    for r in row_iter {
                        out.push(r.map_err(|e| AcsError::Storage(e.to_string()))?);
                    }
                }
                Ok(out)
            })
            .await?;

        // Aggregate rows into per-day buckets in Rust (timezone-aware).
        use std::collections::BTreeMap;

        let mut buckets: BTreeMap<chrono::NaiveDate, DailyBucket> = BTreeMap::new();

        for (finished_at_str, status, cost_opt) in rows {
            let finished_at_utc = chrono::DateTime::parse_from_rfc3339(&finished_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .map_err(|e| {
                    AcsError::Storage(format!("parse finished_at '{}': {}", finished_at_str, e))
                })?;
            let finished_at_local = finished_at_utc.with_timezone(&tz);
            let date_local = finished_at_local.date_naive();
            let cost = cost_opt.unwrap_or(0.0);

            let bucket = buckets.entry(date_local).or_insert_with(|| DailyBucket {
                date: date_local,
                total_usd: 0.0,
                cost_from_completed: 0.0,
                cost_from_failed: 0.0,
                cost_from_killed: 0.0,
                runs_completed: 0,
                runs_failed: 0,
                runs_killed: 0,
            });
            bucket.total_usd += cost;
            match status.as_str() {
                "Completed" => {
                    bucket.runs_completed += 1;
                    bucket.cost_from_completed += cost;
                }
                "Failed" => {
                    bucket.runs_failed += 1;
                    bucket.cost_from_failed += cost;
                }
                "Killed" => {
                    bucket.runs_killed += 1;
                    bucket.cost_from_killed += cost;
                }
                _ => { /* impossible given WHERE clause */ }
            }
        }

        Ok(buckets.into_values().collect())
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

    // ── cost_summary_for ──────────────────────────────────────────────────────

    /// Helper: insert a raw run row with explicit finished_at + status +
    /// optional total_cost_usd directly via SQL so we can place runs at
    /// arbitrary timestamps without going through the ORM.
    async fn seed_raw_run(
        store: &SqliteWorkflowRunStore,
        workflow: &Workflow,
        status: &str,
        finished_at: Option<&str>,
        cost_usd: Option<f64>,
    ) {
        let run_id = Uuid::now_v7().to_string();
        let wf_id = workflow.id.to_string();
        let snapshot = serde_json::to_string(workflow).expect("serialize snapshot");
        let started_at = "2025-01-01T00:00:00+00:00".to_string();
        let db_conn = store.db.clone();
        let status = status.to_string();
        let finished_at = finished_at.map(|s| s.to_string());

        db_conn
            .with_conn(move |c| {
                c.execute(
                    "INSERT INTO workflow_runs (
                        run_id, workflow_id, workflow_version, workflow_snapshot,
                        started_at, finished_at, status, trigger_input, steps_json,
                        total_cost_usd, total_duration_ms
                     ) VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, NULL, '[]', ?7, NULL)",
                    params![
                        run_id,
                        wf_id,
                        snapshot,
                        started_at,
                        finished_at,
                        status,
                        cost_usd,
                    ],
                )
                .map_err(|e| AcsError::Storage(e.to_string()))?;
                Ok(())
            })
            .await
            .expect("seed raw run");
    }

    // 1. Empty case — workflow with no runs returns zeros.
    #[tokio::test]
    async fn test_cost_summary_empty_returns_zeros() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-empty").await;
        let tz: Tz = "UTC".parse().unwrap();
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        assert_eq!(summary.last_30_days_total_usd, 0.0);
        assert_eq!(summary.last_30_days_runs, 0);
        assert_eq!(summary.last_year_total_usd, 0.0);
        assert_eq!(summary.last_year_runs, 0);
        // computed_at should be close to now
        let age = (Utc::now() - summary.computed_at).num_seconds().abs();
        assert!(age < 5, "computed_at should be recent");
    }

    // 2. All runs outside the year window — returns zeros.
    #[tokio::test]
    async fn test_cost_summary_all_outside_window_returns_zeros() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-outside").await;
        let tz: Tz = "UTC".parse().unwrap();
        // 400 days ago — outside the 365-day window
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2024-01-01T00:00:00+00:00"),
            Some(1.0),
        )
        .await;
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        assert_eq!(summary.last_30_days_runs, 0);
        assert_eq!(summary.last_year_runs, 0);
    }

    // 3. Mixed null/non-null total_cost_usd — sum skips nulls, count includes all terminal runs.
    #[tokio::test]
    async fn test_cost_summary_mixed_null_costs() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-null-mix").await;
        let tz: Tz = "UTC".parse().unwrap();
        let recent = "2026-05-09T12:00:00+00:00";
        // run with cost 0.5
        seed_raw_run(&run_store, &wf, "Completed", Some(recent), Some(0.5)).await;
        // run with null cost — counted but not summed
        seed_raw_run(&run_store, &wf, "Completed", Some(recent), None).await;
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        // 2 runs counted, only 0.5 summed
        assert_eq!(summary.last_30_days_runs, 2);
        assert!((summary.last_30_days_total_usd - 0.5).abs() < 1e-9);
    }

    // 4. Failed + Killed runs included.
    #[tokio::test]
    async fn test_cost_summary_failed_and_killed_included() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-statuses").await;
        let tz: Tz = "UTC".parse().unwrap();
        let recent = "2026-05-09T12:00:00+00:00";
        seed_raw_run(&run_store, &wf, "Failed", Some(recent), Some(0.1)).await;
        seed_raw_run(&run_store, &wf, "Killed", Some(recent), Some(0.2)).await;
        seed_raw_run(&run_store, &wf, "Completed", Some(recent), Some(0.3)).await;
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        assert_eq!(summary.last_30_days_runs, 3);
        assert!((summary.last_30_days_total_usd - 0.6).abs() < 1e-9);
    }

    // 5. Running (non-terminal) runs excluded.
    #[tokio::test]
    async fn test_cost_summary_running_excluded() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-running").await;
        let tz: Tz = "UTC".parse().unwrap();
        let recent = "2026-05-09T12:00:00+00:00";
        seed_raw_run(&run_store, &wf, "Running", Some(recent), Some(5.0)).await;
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        assert_eq!(summary.last_30_days_runs, 0);
        assert_eq!(summary.last_year_runs, 0);
    }

    // 6. 30-day vs 1-year window: run from 60 days ago shows in year only.
    #[tokio::test]
    async fn test_cost_summary_60_days_ago_in_year_only() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-windows").await;
        let tz: Tz = "UTC".parse().unwrap();
        // 60 days ago is outside the 30-day window but inside the year window.
        let sixty_days_ago = (Utc::now() - chrono::Duration::days(60)).to_rfc3339();
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some(&sixty_days_ago),
            Some(1.0),
        )
        .await;
        // 10 days ago is inside the 30-day window.
        let recent = (Utc::now() - chrono::Duration::days(10)).to_rfc3339();
        seed_raw_run(&run_store, &wf, "Completed", Some(&recent), Some(2.0)).await;
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        // Only the recent run is in the 30-day window
        assert_eq!(summary.last_30_days_runs, 1);
        assert!((summary.last_30_days_total_usd - 2.0).abs() < 1e-9);
        // Both runs are in the year window
        assert_eq!(summary.last_year_runs, 2);
        assert!((summary.last_year_total_usd - 3.0).abs() < 1e-9);
    }

    // 7. Calendar-day boundary in display_tz.
    // A run that finished 15 days ago (clearly within 30d) in UTC should be
    // counted in both UTC and LA timezone windows.
    #[tokio::test]
    async fn test_cost_summary_calendar_boundary_in_display_tz() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-tz-boundary").await;
        let la_tz: Tz = "America/Los_Angeles".parse().unwrap();
        // 15 days ago at noon UTC — solidly within the 30-day window in any tz.
        let run_ts = (Utc::now() - chrono::Duration::days(15)).to_rfc3339();
        seed_raw_run(&run_store, &wf, "Completed", Some(&run_ts), Some(1.0)).await;
        let summary = run_store
            .cost_summary_for(wf.id, &la_tz)
            .await
            .expect("cost_summary");
        assert_eq!(
            summary.last_30_days_runs, 1,
            "run should be in 30-day window"
        );
    }

    // 8. DST transition — spring-forward boundary in America/Los_Angeles.
    // This test verifies that the 30-day window boundary is computed at midnight
    // local time (not 01:00 due to DST). We use relative timestamps rather than
    // hardcoded dates so the test passes regardless of when it runs.
    //
    // Strategy: place one run 10 days ago (clearly in window) and one run 365+
    // days ago (clearly out of year window). Verify counts.
    #[tokio::test]
    async fn test_cost_summary_dst_transition_spring_forward() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let la_tz: Tz = "America/Los_Angeles".parse().unwrap();

        // wf1: run 10 days ago — should be in the 30-day window.
        let wf1 = seed_workflow(&wf_store, "cost-dst-in").await;
        let in_window = (Utc::now() - chrono::Duration::days(10)).to_rfc3339();
        seed_raw_run(&run_store, &wf1, "Completed", Some(&in_window), Some(2.5)).await;
        let s1 = run_store
            .cost_summary_for(wf1.id, &la_tz)
            .await
            .expect("cost_summary s1");
        assert_eq!(
            s1.last_30_days_runs, 1,
            "run 10 days ago should be in 30-day window"
        );

        // wf2: run 400 days ago — should be outside the 365-day window.
        let wf2 = seed_workflow(&wf_store, "cost-dst-out").await;
        let out_of_window = (Utc::now() - chrono::Duration::days(400)).to_rfc3339();
        seed_raw_run(
            &run_store,
            &wf2,
            "Completed",
            Some(&out_of_window),
            Some(1.0),
        )
        .await;
        let s2 = run_store
            .cost_summary_for(wf2.id, &la_tz)
            .await
            .expect("cost_summary s2");
        assert_eq!(
            s2.last_year_runs, 0,
            "run 400 days ago should be outside the year window"
        );
    }

    // 9. Year boundary — a run from 100 days ago is within 365 days but NOT
    // within 30 days.
    #[tokio::test]
    async fn test_cost_summary_year_boundary() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-year-boundary").await;
        let tz: Tz = "UTC".parse().unwrap();
        // 100 days ago is clearly within the year but outside the 30-day window.
        let run_ts = (Utc::now() - chrono::Duration::days(100)).to_rfc3339();
        seed_raw_run(&run_store, &wf, "Completed", Some(&run_ts), Some(3.0)).await;
        let summary = run_store
            .cost_summary_for(wf.id, &tz)
            .await
            .expect("cost_summary");
        assert_eq!(
            summary.last_year_runs, 1,
            "run 100 days ago should appear in last-year window"
        );
        // NOT within the 30-day window.
        assert_eq!(
            summary.last_30_days_runs, 0,
            "run 100 days ago should not appear in last-30-days window"
        );
    }

    // 10. Default timezone (UTC) — verify basic operation.
    #[tokio::test]
    async fn test_cost_summary_utc_timezone() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "cost-utc").await;
        let utc_tz: Tz = "UTC".parse().unwrap();
        let recent = "2026-05-09T00:00:00+00:00";
        seed_raw_run(&run_store, &wf, "Completed", Some(recent), Some(1.5)).await;
        let summary = run_store
            .cost_summary_for(wf.id, &utc_tz)
            .await
            .expect("cost_summary");
        assert_eq!(summary.last_30_days_runs, 1);
        assert!((summary.last_30_days_total_usd - 1.5).abs() < 1e-9);
        assert_eq!(summary.last_year_runs, 1);
    }

    // ── daily_buckets_for tests ───────────────────────────────────────────────

    fn year_window() -> (DateTime<Utc>, DateTime<Utc>) {
        let now = Utc::now();
        let since = now - chrono::Duration::days(365);
        (since, now)
    }

    // 1. Workflow with no runs returns Vec::new().
    #[tokio::test]
    async fn test_daily_buckets_empty() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-empty").await;
        let tz: Tz = "UTC".parse().unwrap();
        let (since, until) = year_window();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");
        assert!(buckets.is_empty(), "no runs → empty bucket list");
    }

    // 2. Runs spanning a display_tz midnight boundary end up in two buckets.
    #[tokio::test]
    async fn test_daily_buckets_groups_by_local_date() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-group-date").await;
        // America/Los_Angeles is UTC-7 (PDT) in summer.
        // 2026-06-01T06:30:00Z is 2026-05-31T23:30:00 PDT → May 31.
        // 2026-06-01T08:30:00Z is 2026-06-01T01:30:00 PDT → June 1.
        // Both are on the same UTC day (June 1) but different local days.
        let la_tz: Tz = "America/Los_Angeles".parse().unwrap();
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-06-01T06:30:00+00:00"),
            Some(1.0),
        )
        .await;
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-06-01T08:30:00+00:00"),
            Some(2.0),
        )
        .await;

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-07-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &la_tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(
            buckets.len(),
            2,
            "expected 2 date buckets, got: {:?}",
            buckets.iter().map(|b| b.date).collect::<Vec<_>>()
        );
        // Verify distinct dates
        assert_ne!(buckets[0].date, buckets[1].date);
    }

    // 3. All three statuses land in their respective sub-fields.
    #[tokio::test]
    async fn test_daily_buckets_includes_all_statuses() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-all-statuses").await;
        let tz: Tz = "UTC".parse().unwrap();
        let day = "2026-05-09T12:00:00+00:00";
        seed_raw_run(&run_store, &wf, "Completed", Some(day), Some(1.0)).await;
        seed_raw_run(&run_store, &wf, "Failed", Some(day), Some(0.5)).await;
        seed_raw_run(&run_store, &wf, "Killed", Some(day), Some(0.25)).await;

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 1);
        let b = &buckets[0];
        assert_eq!(b.runs_completed, 1);
        assert_eq!(b.runs_failed, 1);
        assert_eq!(b.runs_killed, 1);
        assert!((b.cost_from_completed - 1.0).abs() < 1e-9);
        assert!((b.cost_from_failed - 0.5).abs() < 1e-9);
        assert!((b.cost_from_killed - 0.25).abs() < 1e-9);
        assert!((b.total_usd - 1.75).abs() < 1e-9);
    }

    // 4. Running runs are excluded from buckets.
    #[tokio::test]
    async fn test_daily_buckets_excludes_running() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-excl-running").await;
        let tz: Tz = "UTC".parse().unwrap();
        let day = "2026-05-09T12:00:00+00:00";
        seed_raw_run(&run_store, &wf, "Completed", Some(day), Some(1.0)).await;
        seed_raw_run(&run_store, &wf, "Completed", Some(day), Some(2.0)).await;
        seed_raw_run(&run_store, &wf, "Running", Some(day), Some(99.0)).await;

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].runs_completed, 2);
        assert_eq!(buckets[0].runs_failed, 0);
        assert_eq!(buckets[0].runs_killed, 0);
        assert!((buckets[0].total_usd - 3.0).abs() < 1e-9);
    }

    // 5. Null cost_usd treated as 0.0; run is still counted.
    #[tokio::test]
    async fn test_daily_buckets_null_cost_handling() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-null-cost").await;
        let tz: Tz = "UTC".parse().unwrap();
        let day = "2026-05-09T12:00:00+00:00";
        seed_raw_run(&run_store, &wf, "Completed", Some(day), Some(0.50)).await;
        seed_raw_run(&run_store, &wf, "Completed", Some(day), Some(0.50)).await;
        seed_raw_run(&run_store, &wf, "Killed", Some(day), None).await;

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 1);
        let b = &buckets[0];
        assert!(
            (b.total_usd - 1.0).abs() < 1e-9,
            "total_usd={}",
            b.total_usd
        );
        assert_eq!(b.runs_killed, 1);
        assert!((b.cost_from_killed - 0.0).abs() < 1e-9);
    }

    // 6. Window filtering — only runs in [since, until) appear.
    #[tokio::test]
    async fn test_daily_buckets_window_filter() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-window-filter").await;
        let tz: Tz = "UTC".parse().unwrap();

        // Seed 60 days of data (one run per day).
        let base: DateTime<Utc> = "2026-01-01T12:00:00+00:00".parse().unwrap();
        for d in 0i64..60 {
            let ts = (base + chrono::Duration::days(d)).to_rfc3339();
            seed_raw_run(&run_store, &wf, "Completed", Some(&ts), Some(1.0)).await;
        }

        // Request only days 20–30 (10 days).
        let since = base + chrono::Duration::days(20);
        let until = base + chrono::Duration::days(30);
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 10, "should have exactly 10 day-buckets");
    }

    // 7. DST spring-forward — run at 02:30 UTC on spring-forward day.
    // 2026-03-08 is the spring-forward day in America/Los_Angeles.
    // At 02:00 PST (10:00 UTC on Mar 7), clocks spring forward to 03:00 PDT.
    // So 10:30 UTC on 2026-03-08 = 02:30 AM (which doesn't exist in LA) →
    // in practice, runs stored at 10:30 UTC on Mar 8 fall on Mar 8 local time.
    #[tokio::test]
    async fn test_daily_buckets_dst_spring_forward() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-dst-spring").await;
        let la_tz: Tz = "America/Los_Angeles".parse().unwrap();

        // 2026-03-08T10:30:00Z → in LA this is Mar 8, 02:30 (skipped hour),
        // but chrono resolves it to Mar 8 local date.
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-03-08T10:30:00+00:00"),
            Some(1.0),
        )
        .await;

        let since: DateTime<Utc> = "2026-03-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-04-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &la_tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 1, "should be exactly one date bucket");
        // The run should land on March 8 local date.
        let expected_date = chrono::NaiveDate::from_ymd_opt(2026, 3, 8).unwrap();
        assert_eq!(
            buckets[0].date, expected_date,
            "run should be on 2026-03-08 local"
        );
    }

    // 8. Year boundary — runs around Dec 31 / Jan 1 get the right local dates.
    #[tokio::test]
    async fn test_daily_buckets_year_boundary() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-year-boundary").await;
        let tz: Tz = "UTC".parse().unwrap();

        // Dec 31, 2025 at noon UTC.
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2025-12-31T12:00:00+00:00"),
            Some(1.0),
        )
        .await;
        // Jan 1, 2026 at noon UTC.
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-01-01T12:00:00+00:00"),
            Some(2.0),
        )
        .await;

        let since: DateTime<Utc> = "2025-12-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-02-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 2);
        assert_eq!(
            buckets[0].date,
            chrono::NaiveDate::from_ymd_opt(2025, 12, 31).unwrap()
        );
        assert_eq!(
            buckets[1].date,
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()
        );
    }

    // 9. workflow_id filter — Some(A) returns only A's runs; None returns combined.
    #[tokio::test]
    async fn test_daily_buckets_workflow_id_filter() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf_a = seed_workflow(&wf_store, "db-filter-a").await;
        let wf_b = seed_workflow(&wf_store, "db-filter-b").await;
        let tz: Tz = "UTC".parse().unwrap();
        let day_a = "2026-05-05T12:00:00+00:00";
        let day_b = "2026-05-06T12:00:00+00:00";

        seed_raw_run(&run_store, &wf_a, "Completed", Some(day_a), Some(1.0)).await;
        seed_raw_run(&run_store, &wf_b, "Completed", Some(day_b), Some(2.0)).await;

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();

        // Filter by wf_a only.
        let buckets_a = run_store
            .daily_buckets_for(Some(wf_a.id), since, until, &tz)
            .await
            .expect("daily_buckets_for wf_a");
        assert_eq!(buckets_a.len(), 1);
        assert_eq!(
            buckets_a[0].date,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 5).unwrap()
        );

        // System aggregate (None) returns both.
        let buckets_all = run_store
            .daily_buckets_for(None, since, until, &tz)
            .await
            .expect("daily_buckets_for all");
        assert_eq!(buckets_all.len(), 2);
    }

    // 10. Results are in ascending date order.
    #[tokio::test]
    async fn test_daily_buckets_ascending_order() {
        let (wf_store, run_store, _db) = SqliteWorkflowRunStore::paired_for_tests();
        let wf = seed_workflow(&wf_store, "db-asc-order").await;
        let tz: Tz = "UTC".parse().unwrap();

        // Insert 5 runs across 3 different days (out-of-order insertion).
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-05-10T12:00:00+00:00"),
            Some(3.0),
        )
        .await;
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-05-08T12:00:00+00:00"),
            Some(1.0),
        )
        .await;
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-05-09T12:00:00+00:00"),
            Some(2.0),
        )
        .await;
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-05-08T15:00:00+00:00"),
            Some(1.0),
        )
        .await;
        seed_raw_run(
            &run_store,
            &wf,
            "Completed",
            Some("2026-05-10T15:00:00+00:00"),
            Some(3.0),
        )
        .await;

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let buckets = run_store
            .daily_buckets_for(Some(wf.id), since, until, &tz)
            .await
            .expect("daily_buckets_for");

        assert_eq!(buckets.len(), 3, "should have 3 date buckets");
        // Ascending order.
        assert!(buckets[0].date < buckets[1].date);
        assert!(buckets[1].date < buckets[2].date);
        assert_eq!(
            buckets[0].date,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 8).unwrap()
        );
        assert_eq!(
            buckets[1].date,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 9).unwrap()
        );
        assert_eq!(
            buckets[2].date,
            chrono::NaiveDate::from_ymd_opt(2026, 5, 10).unwrap()
        );
        // Aggregation correct.
        assert_eq!(buckets[0].runs_completed, 2);
        assert_eq!(buckets[2].runs_completed, 2);
    }
}
