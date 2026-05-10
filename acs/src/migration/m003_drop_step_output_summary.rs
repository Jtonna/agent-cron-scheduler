//! Migration 003 — Drop the legacy `output_summary` field from every
//! `StepRun` JSON record persisted in `workflow_runs.steps_json`.
//!
//! Per-step output now lives exclusively in
//! `<data_dir>/logs/<workflow_id>/<run_id>.log`, framed by each `StepRun`'s
//! `log_byte_offset_start` / `log_byte_offset_end` pair. The inline copy
//! that older runs persisted alongside those offsets is redundant — this
//! migration walks every row in `workflow_runs`, parses the `steps_json`
//! array, removes any `output_summary` keys it finds, and writes the
//! stripped JSON back. The whole pass runs inside a single transaction so
//! a parse/serialise failure aborts cleanly without leaving the table half
//! migrated.
//!
//! # Idempotency
//!
//! Re-running on a database that contains no `output_summary` keys is a
//! no-op: the migration tallies how many step records had the key and
//! returns `Ok(false)` when the count is zero, so the runner records
//! nothing in `migrations.json` and the row blobs are not rewritten.
//! Only when at least one row carried the key is `Ok(true)` returned.
//!
//! # Fresh installs
//!
//! On a fresh install `acs.db` may not exist yet — this migration runs
//! after `m002_json_to_sqlite` in the registry, but `m002` only creates
//! the DB when JSON sources or fresh-install conditions apply. If the
//! file is missing here it means there is nothing to migrate; we return
//! `Ok(false)` rather than failing.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::params;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

const STEP_OUTPUT_SUMMARY_KEY: &str = "output_summary";

/// Migration 003: strip `output_summary` from every persisted `StepRun`.
pub struct DropStepOutputSummary;

#[async_trait]
impl Migration for DropStepOutputSummary {
    fn name(&self) -> &'static str {
        "m003_drop_step_output_summary"
    }

    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError> {
        let db_path = data_dir.join("acs.db");
        if !db_path.exists() {
            // No DB → nothing to migrate. m002 normally creates this on a
            // fresh install; absence here means no run history to rewrite.
            return Ok(false);
        }

        let db_path_clone = db_path.clone();
        tokio::task::spawn_blocking(move || run_blocking(&db_path_clone))
            .await
            .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?
    }
}

fn run_blocking(db_path: &Path) -> Result<bool, AcsError> {
    let mut conn = sqlite::open_with_schema(db_path)?;

    let tx = conn
        .transaction()
        .map_err(|e| AcsError::Storage(format!("Failed to start transaction: {}", e)))?;

    // Snapshot every (run_id, steps_json) pair up front. The rows are small
    // enough that buffering them is cheaper than juggling an open SELECT and
    // an UPDATE on the same connection inside the transaction.
    let rows: Vec<(String, String)> = {
        let mut stmt = tx
            .prepare("SELECT run_id, steps_json FROM workflow_runs")
            .map_err(|e| AcsError::Storage(format!("Failed to prepare SELECT: {}", e)))?;
        let mapped = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AcsError::Storage(format!("Failed to execute SELECT: {}", e)))?;
        let mut out = Vec::new();
        for r in mapped {
            out.push(r.map_err(|e| AcsError::Storage(format!("Row read failed: {}", e)))?);
        }
        out
    };

    // Run the row rewrite in an inner closure so any error path can roll back
    // the transaction explicitly. If the closure returns Ok with rewritten=0
    // we still want to drop the read-only tx and return Ok(false); on Ok with
    // rewritten>0 we COMMIT.
    let result: Result<usize, AcsError> = (|| {
        let mut rewritten_rows: usize = 0;
        for (run_id, steps_json) in rows {
            let mut value: serde_json::Value = serde_json::from_str(&steps_json).map_err(|e| {
                AcsError::Storage(format!(
                    "Failed to parse steps_json for run {}: {}",
                    run_id, e
                ))
            })?;

            let stripped = strip_output_summary(&mut value);
            if stripped == 0 {
                continue;
            }

            let new_json = serde_json::to_string(&value).map_err(|e| {
                AcsError::Storage(format!(
                    "Failed to re-serialise steps_json for run {}: {}",
                    run_id, e
                ))
            })?;

            tx.execute(
                "UPDATE workflow_runs SET steps_json = ?1 WHERE run_id = ?2",
                params![new_json, run_id],
            )
            .map_err(|e| AcsError::Storage(format!("UPDATE for run {} failed: {}", run_id, e)))?;

            rewritten_rows += 1;
        }
        Ok(rewritten_rows)
    })();

    let rewritten_rows = match result {
        Ok(n) => n,
        Err(e) => {
            if let Err(rb) = tx.rollback() {
                tracing::warn!(
                    "m003_drop_step_output_summary: rollback after error also failed: {}",
                    rb
                );
            }
            return Err(e);
        }
    };

    if rewritten_rows == 0 {
        // Nothing to do — drop the (read-only) transaction without committing
        // and report the migration as not-needed so the runner does not write
        // it into the applied set.
        if let Err(e) = tx.rollback() {
            tracing::warn!(
                "m003_drop_step_output_summary: rollback of read-only transaction failed: {}",
                e
            );
        }
        return Ok(false);
    }

    tx.commit()
        .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;

    tracing::info!(
        "Migration m003 complete: stripped output_summary from {} workflow_runs row(s)",
        rewritten_rows
    );
    Ok(true)
}

/// Strip every `output_summary` key from a `steps_json` blob.
///
/// The schema is flat: `steps_json` is a JSON array, each element is a JSON
/// object, and each object MAY carry an `output_summary` key at depth 1.
/// A single 2-level pass (outer array → inner object → `map.remove`) is
/// sufficient — no recursion needed. Non-array / non-object inputs are
/// treated as a no-op.
///
/// Returns the total number of `output_summary` keys removed.
fn strip_output_summary(value: &mut serde_json::Value) -> usize {
    let Some(items) = value.as_array_mut() else {
        return 0;
    };
    let mut removed = 0;
    for item in items.iter_mut() {
        if let Some(map) = item.as_object_mut() {
            if map.remove(STEP_OUTPUT_SUMMARY_KEY).is_some() {
                removed += 1;
            }
        }
    }
    removed
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Insert a workflow row that satisfies the FOREIGN KEY on workflow_runs.
    fn insert_workflow(conn: &rusqlite::Connection, id: Uuid) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO workflows (
                id, name, version, schedule, timezone, schedule_mode, enabled,
                steps_json, default_input, working_dir, env_vars,
                allow_concurrent, on_failure, last_run_at, last_run_status,
                last_run_id, created_at, updated_at
            ) VALUES (
                ?1, ?2, 1, '* * * * *', NULL, 'Cron', 1,
                '[]', NULL, NULL, NULL,
                1, '\"abort\"', NULL, NULL,
                NULL, ?3, ?3
            )",
            params![id.to_string(), format!("wf-{}", id), now],
        )
        .expect("insert workflow");
    }

    fn insert_run(conn: &rusqlite::Connection, run_id: Uuid, workflow_id: Uuid, steps_json: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        // workflow_snapshot must parse as JSON; a minimal stub is fine because
        // the migration only touches steps_json.
        let snapshot = serde_json::json!({
            "id": workflow_id.to_string(),
            "name": format!("wf-{}", workflow_id),
            "version": 1,
            "schedule": "* * * * *",
            "enabled": true,
            "steps": [],
            "allow_concurrent": true,
            "on_failure": "abort",
            "schedule_mode": "Cron",
            "created_at": now,
            "updated_at": now,
        });
        conn.execute(
            "INSERT INTO workflow_runs (
                run_id, workflow_id, workflow_version, workflow_snapshot,
                started_at, finished_at, status, trigger_input, steps_json,
                total_cost_usd, total_duration_ms
            ) VALUES (?1, ?2, 1, ?3, ?4, NULL, 'Running', NULL, ?5, NULL, NULL)",
            params![
                run_id.to_string(),
                workflow_id.to_string(),
                serde_json::to_string(&snapshot).unwrap(),
                now,
                steps_json,
            ],
        )
        .expect("insert workflow_run");
    }

    fn read_steps_json(db_path: &Path, run_id: Uuid) -> String {
        let conn = sqlite::open_with_schema(db_path).expect("open");
        conn.query_row(
            "SELECT steps_json FROM workflow_runs WHERE run_id = ?1",
            [run_id.to_string()],
            |r| r.get::<_, String>(0),
        )
        .expect("query steps_json")
    }

    // ── Strips output_summary across runs that still carry it ────────────────

    #[tokio::test]
    async fn test_strips_output_summary_when_present() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let wf_id = Uuid::now_v7();
            insert_workflow(&conn, wf_id);

            let run_id = Uuid::now_v7();
            let legacy_steps = serde_json::json!([
                {
                    "step_index": 0,
                    "step_id": "s0",
                    "kind": "shell",
                    "status": "Completed",
                    "started_at": "2025-01-01T00:00:00Z",
                    "log_byte_offset_start": 0,
                    "output_summary": {"stdout": "old captured bytes"}
                },
                {
                    "step_index": 1,
                    "step_id": "s1",
                    "kind": "shell",
                    "status": "Completed",
                    "started_at": "2025-01-01T00:00:01Z",
                    "log_byte_offset_start": 100,
                    "output_summary": "raw string output"
                }
            ]);
            insert_run(&conn, run_id, wf_id, &legacy_steps.to_string());

            let m = DropStepOutputSummary;
            let did_work = m.run(tmp.path()).await.expect("migrate");
            assert!(
                did_work,
                "expected Ok(true) when at least one row carried output_summary"
            );

            // Each step record should now lack the field.
            let steps_after = read_steps_json(&db_path, run_id);
            let parsed: serde_json::Value = serde_json::from_str(&steps_after).unwrap();
            for elem in parsed.as_array().unwrap() {
                assert!(
                    elem.get("output_summary").is_none(),
                    "output_summary must be stripped, but row was: {}",
                    elem
                );
            }
            // Sanity: other fields preserved.
            assert_eq!(parsed[0]["step_id"], "s0");
            assert_eq!(parsed[1]["log_byte_offset_start"], 100);
        }
    }

    // ── Idempotent: already-stripped rows produce Ok(false) ─────────────────

    #[tokio::test]
    async fn test_returns_false_when_nothing_to_strip() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        let conn = sqlite::open_with_schema(&db_path).expect("open");
        let wf_id = Uuid::now_v7();
        insert_workflow(&conn, wf_id);
        let run_id = Uuid::now_v7();
        let modern_steps = serde_json::json!([
            {
                "step_index": 0,
                "step_id": "s0",
                "kind": "shell",
                "status": "Completed",
                "started_at": "2025-01-01T00:00:00Z",
                "log_byte_offset_start": 0
            }
        ]);
        insert_run(&conn, run_id, wf_id, &modern_steps.to_string());
        drop(conn);

        let m = DropStepOutputSummary;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "expected Ok(false) when nothing to strip");

        // Steps_json must be byte-identical to what we wrote.
        let after = read_steps_json(&db_path, run_id);
        assert_eq!(after, modern_steps.to_string());
    }

    // ── Idempotent: empty workflow_runs table is a trivial no-op ────────────

    #[tokio::test]
    async fn test_empty_table_returns_false() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        // Just create the schema; no rows.
        let _conn = sqlite::open_with_schema(&db_path).expect("open");
        drop(_conn);

        let m = DropStepOutputSummary;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "empty table is a no-op");
    }

    // ── Re-running after a successful pass is also a no-op ──────────────────

    #[tokio::test]
    async fn test_second_run_is_noop() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let wf_id = Uuid::now_v7();
            insert_workflow(&conn, wf_id);
            let run_id = Uuid::now_v7();
            let legacy_steps = serde_json::json!([
                {
                    "step_index": 0,
                    "step_id": "s0",
                    "kind": "shell",
                    "status": "Completed",
                    "started_at": "2025-01-01T00:00:00Z",
                    "log_byte_offset_start": 0,
                    "output_summary": {"x": 1}
                }
            ]);
            insert_run(&conn, run_id, wf_id, &legacy_steps.to_string());
        }
        let m = DropStepOutputSummary;
        let first = m.run(tmp.path()).await.expect("first");
        assert!(first, "first run rewrites the row");
        let second = m.run(tmp.path()).await.expect("second");
        assert!(!second, "second run finds nothing to do");
    }

    // ── Fresh install (no acs.db) is a no-op ────────────────────────────────

    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = DropStepOutputSummary;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db is a no-op");
    }

    // ── Parse failure rolls back and propagates ──────────────────────────────

    #[tokio::test]
    async fn test_corrupt_steps_json_aborts_migration() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            let wf_id = Uuid::now_v7();
            insert_workflow(&conn, wf_id);
            let run_id = Uuid::now_v7();
            // Write a deliberately malformed steps_json blob.
            insert_run(&conn, run_id, wf_id, "not valid json {{{");
        }
        let m = DropStepOutputSummary;
        let result = m.run(tmp.path()).await;
        assert!(result.is_err(), "corrupt JSON must surface as an error");
    }
}
