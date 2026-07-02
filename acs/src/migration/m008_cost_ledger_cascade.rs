//! Migration 008 — durable cost ledger + `ON DELETE CASCADE` on
//! `workflow_runs.workflow_id`.
//!
//! # Background
//!
//! v4.2.14 fixes `DELETE /api/workflows/{id}` returning a 500
//! (`FOREIGN KEY constraint failed`) for any workflow with run history, and
//! introduces the `cost_ledger` table so cost/token bookkeeping survives
//! deletion of workflows and runs.
//!
//! # What this migration does
//!
//! If `acs.db` does not exist → return `Ok(false)` (fresh install — the
//! ledger table and the CASCADE clause are present from the start via
//! `schema.rs`).
//!
//! If the `workflow_runs.workflow_id` FK already has `ON DELETE CASCADE` →
//! return `Ok(false)` (idempotent).
//!
//! Otherwise:
//!
//! 1. Backfill `cost_ledger` from every terminal (`Completed` / `Failed` /
//!    `Killed`) row in `workflow_runs`, extracting `workflow_name` from the
//!    stored workflow snapshot. `INSERT OR IGNORE` keeps re-runs safe.
//! 2. Rebuild `workflow_runs` with `ON DELETE CASCADE` on the FK. SQLite
//!    cannot alter an existing FK, so this is the standard table rebuild:
//!    create new table → copy rows → drop old → rename → recreate indexes.
//!    The rebuild runs with `PRAGMA foreign_keys = OFF` (a no-op inside a
//!    transaction, so it is toggled outside of one) and everything else in a
//!    single transaction.
//!
//! The `cost_ledger` table itself is created by `schema.rs` (`apply_schema`
//! runs on every open, and `open_with_schema` below applies it) — this
//! migration only needs to populate it and rebuild the runs table.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::Connection;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

pub struct CostLedgerCascade;

#[async_trait]
impl Migration for CostLedgerCascade {
    fn name(&self) -> &'static str {
        "m008_cost_ledger_cascade"
    }

    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError> {
        let db_path = data_dir.join("acs.db");
        if !db_path.exists() {
            return Ok(false);
        }
        let db_path_clone = db_path.clone();
        tokio::task::spawn_blocking(move || run_blocking(&db_path_clone))
            .await
            .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?
    }
}

/// Return `true` when the `workflow_runs.workflow_id` FK already declares
/// `ON DELETE CASCADE`.
fn fk_has_cascade(conn: &Connection) -> Result<bool, AcsError> {
    // PRAGMA foreign_key_list returns one row per FK with fields:
    //   id | seq | table | from | to | on_update | on_delete | match
    let mut stmt = conn
        .prepare("PRAGMA foreign_key_list(workflow_runs)")
        .map_err(|e| {
            AcsError::Storage(format!("Failed to prepare PRAGMA foreign_key_list: {}", e))
        })?;
    let on_deletes: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(6))
        .map_err(|e| AcsError::Storage(format!("Failed to query PRAGMA foreign_key_list: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AcsError::Storage(format!("Failed to collect FK rows: {}", e)))?;
    Ok(on_deletes.iter().any(|d| d == "CASCADE"))
}

fn run_blocking(db_path: &Path) -> Result<bool, AcsError> {
    // open_with_schema applies pragmas and the current schema — this creates
    // the (empty) cost_ledger table on databases from older versions.
    let mut conn = sqlite::open_with_schema(db_path)?;

    // Idempotency check: the CASCADE clause is the last thing this migration
    // installs, so its presence means the whole migration already ran (or the
    // DB was created fresh with the current schema).
    if fk_has_cascade(&conn)? {
        return Ok(false);
    }

    // The table rebuild must not fire FK checks mid-flight. PRAGMA
    // foreign_keys is a no-op inside a transaction, so toggle it outside.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| AcsError::Storage(format!("Failed to disable foreign_keys: {}", e)))?;

    let result = (|| -> Result<(usize, usize), AcsError> {
        let tx = conn
            .transaction()
            .map_err(|e| AcsError::Storage(format!("Failed to start transaction: {}", e)))?;

        // 1. Backfill the ledger from existing terminal runs. workflow_name
        //    comes from the persisted workflow snapshot JSON.
        let backfilled = tx
            .execute(
                "INSERT OR IGNORE INTO cost_ledger (
                    run_id, workflow_id, workflow_name, started_at, finished_at,
                    status, total_cost_usd, total_input_tokens, total_output_tokens
                 )
                 SELECT run_id,
                        workflow_id,
                        COALESCE(json_extract(workflow_snapshot, '$.name'), ''),
                        started_at,
                        finished_at,
                        status,
                        total_cost_usd,
                        COALESCE(total_input_tokens, 0),
                        COALESCE(total_output_tokens, 0)
                 FROM workflow_runs
                 WHERE status IN ('Completed', 'Failed', 'Killed')",
                [],
            )
            .map_err(|e| AcsError::Storage(format!("cost_ledger backfill failed: {}", e)))?;

        // 2. Rebuild workflow_runs with ON DELETE CASCADE.
        tx.execute_batch(
            "CREATE TABLE workflow_runs_new (
                run_id              TEXT PRIMARY KEY,
                workflow_id         TEXT NOT NULL,
                workflow_version    INTEGER NOT NULL,
                workflow_snapshot   TEXT NOT NULL,
                started_at          TEXT NOT NULL,
                finished_at         TEXT,
                status              TEXT NOT NULL,
                trigger_input       TEXT,
                steps_json          TEXT NOT NULL,
                total_cost_usd         REAL,
                total_duration_ms      INTEGER,
                total_input_tokens     INTEGER NOT NULL DEFAULT 0,
                total_output_tokens    INTEGER NOT NULL DEFAULT 0,
                FOREIGN KEY (workflow_id) REFERENCES workflows(id) ON DELETE CASCADE
             );
             INSERT INTO workflow_runs_new (
                run_id, workflow_id, workflow_version, workflow_snapshot,
                started_at, finished_at, status, trigger_input, steps_json,
                total_cost_usd, total_duration_ms,
                total_input_tokens, total_output_tokens
             )
             SELECT run_id, workflow_id, workflow_version, workflow_snapshot,
                    started_at, finished_at, status, trigger_input, steps_json,
                    total_cost_usd, total_duration_ms,
                    total_input_tokens, total_output_tokens
             FROM workflow_runs;
             DROP TABLE workflow_runs;
             ALTER TABLE workflow_runs_new RENAME TO workflow_runs;
             CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow_id_finished_at
                 ON workflow_runs(workflow_id, finished_at);
             CREATE INDEX IF NOT EXISTS idx_workflow_runs_finished_at
                 ON workflow_runs(finished_at);
             CREATE INDEX IF NOT EXISTS idx_workflow_runs_status
                 ON workflow_runs(status);",
        )
        .map_err(|e| AcsError::Storage(format!("workflow_runs rebuild failed: {}", e)))?;

        let copied: usize = tx
            .query_row("SELECT COUNT(*) FROM workflow_runs", [], |r| {
                r.get::<_, i64>(0)
            })
            .map(|n| n as usize)
            .map_err(|e| AcsError::Storage(format!("post-rebuild count failed: {}", e)))?;

        tx.commit()
            .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;
        Ok((backfilled, copied))
    })();

    // Always restore the FK pragma, even when the transaction failed.
    let restore = conn
        .execute_batch("PRAGMA foreign_keys = ON;")
        .map_err(|e| AcsError::Storage(format!("Failed to re-enable foreign_keys: {}", e)));

    let (backfilled, copied) = result?;
    restore?;

    tracing::info!(
        "Migration m008 complete: backfilled {} cost_ledger row(s) and rebuilt \
         workflow_runs ({} row(s)) with ON DELETE CASCADE",
        backfilled,
        copied
    );
    Ok(true)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    /// DDL matching a v4.2.13 database: token columns present (m007 applied),
    /// no cost_ledger table, no CASCADE on the FK.
    const OLD_SCHEMA: &str = "
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA synchronous = NORMAL;
        CREATE TABLE IF NOT EXISTS workflows (
            id TEXT PRIMARY KEY, name TEXT NOT NULL UNIQUE,
            version INTEGER NOT NULL, schedule TEXT NOT NULL,
            timezone TEXT, schedule_mode TEXT NOT NULL,
            enabled INTEGER NOT NULL, steps_json TEXT NOT NULL,
            default_input TEXT, working_dir TEXT, env_vars TEXT,
            allow_concurrent INTEGER NOT NULL, on_failure TEXT NOT NULL,
            last_run_at TEXT, last_run_status TEXT, last_run_id TEXT,
            created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            is_favorited INTEGER NOT NULL DEFAULT 0
        );
        CREATE TABLE IF NOT EXISTS workflow_runs (
            run_id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL,
            workflow_version INTEGER NOT NULL, workflow_snapshot TEXT NOT NULL,
            started_at TEXT NOT NULL, finished_at TEXT,
            status TEXT NOT NULL, trigger_input TEXT,
            steps_json TEXT NOT NULL, total_cost_usd REAL,
            total_duration_ms INTEGER,
            total_input_tokens INTEGER NOT NULL DEFAULT 0,
            total_output_tokens INTEGER NOT NULL DEFAULT 0,
            FOREIGN KEY (workflow_id) REFERENCES workflows(id)
        );
        CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow_id_finished_at
            ON workflow_runs(workflow_id, finished_at);
        CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);";

    fn seed_old_db(db_path: &Path) {
        let conn = Connection::open(db_path).expect("open");
        conn.execute_batch(OLD_SCHEMA).expect("apply old schema");
        conn.execute_batch(
            "INSERT INTO workflows (id, name, version, schedule, schedule_mode, enabled,
                steps_json, allow_concurrent, on_failure, created_at, updated_at)
             VALUES ('wf-1', 'legacy-wf', 1, '* * * * *', 'cron', 1, '[]', 1, 'abort',
                '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');
             INSERT INTO workflow_runs (run_id, workflow_id, workflow_version,
                workflow_snapshot, started_at, finished_at, status, steps_json,
                total_cost_usd, total_input_tokens, total_output_tokens)
             VALUES
                ('run-1', 'wf-1', 1, '{\"name\":\"legacy-wf\"}',
                 '2026-01-01T00:00:00Z', '2026-01-01T00:01:00Z', 'Completed', '[]',
                 0.25, 100, 50),
                ('run-2', 'wf-1', 1, '{\"name\":\"legacy-wf\"}',
                 '2026-01-02T00:00:00Z', '2026-01-02T00:01:00Z', 'Failed', '[]',
                 NULL, 0, 0),
                ('run-3', 'wf-1', 1, '{\"name\":\"legacy-wf\"}',
                 '2026-01-03T00:00:00Z', NULL, 'Running', '[]',
                 NULL, 0, 0);",
        )
        .expect("seed rows");
    }

    // ── Test 1: no acs.db → no-op ─────────────────────────────────────────────

    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = CostLedgerCascade;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db must be a no-op");
    }

    // ── Test 2: fresh DB created with the current schema → no-op ─────────────

    #[tokio::test]
    async fn test_fresh_db_with_cascade_is_noop() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        // Fresh DB via the current schema — CASCADE is already present.
        let _db = crate::storage::sqlite::init_db(&db_path).expect("init");
        let m = CostLedgerCascade;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "fresh DB already has CASCADE → no-op");
    }

    // ── Test 3: old DB → backfills ledger and installs CASCADE ────────────────

    #[tokio::test]
    async fn test_backfills_ledger_and_installs_cascade() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        seed_old_db(&db_path);

        let m = CostLedgerCascade;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "old DB must be migrated");

        let conn = crate::storage::sqlite::open_with_schema(&db_path).expect("reopen");

        // Ledger contains only the two terminal runs.
        let ledger_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM cost_ledger", [], |r| r.get(0))
            .expect("count ledger");
        assert_eq!(ledger_count, 2, "only terminal runs are backfilled");

        // workflow_name extracted from the snapshot.
        let name: String = conn
            .query_row(
                "SELECT workflow_name FROM cost_ledger WHERE run_id = 'run-1'",
                [],
                |r| r.get(0),
            )
            .expect("ledger name");
        assert_eq!(name, "legacy-wf");

        // Cost + tokens preserved.
        let (cost, in_tok, out_tok): (f64, i64, i64) = conn
            .query_row(
                "SELECT total_cost_usd, total_input_tokens, total_output_tokens \
                 FROM cost_ledger WHERE run_id = 'run-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .expect("ledger row");
        assert!((cost - 0.25).abs() < 1e-9);
        assert_eq!(in_tok, 100);
        assert_eq!(out_tok, 50);

        // All three run rows survived the rebuild.
        let run_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM workflow_runs", [], |r| r.get(0))
            .expect("count runs");
        assert_eq!(run_count, 3);

        // FK now cascades.
        assert!(
            fk_has_cascade(&conn).expect("fk check"),
            "workflow_runs FK must be ON DELETE CASCADE after migration"
        );

        // ...and it actually works: deleting the workflow removes its runs.
        conn.execute("DELETE FROM workflows WHERE id = 'wf-1'", [])
            .expect("delete workflow");
        let remaining: i64 = conn
            .query_row("SELECT COUNT(*) FROM workflow_runs", [], |r| r.get(0))
            .expect("count runs after delete");
        assert_eq!(remaining, 0, "CASCADE must delete child runs");

        // The ledger is untouched by the cascade.
        let ledger_after: i64 = conn
            .query_row("SELECT COUNT(*) FROM cost_ledger", [], |r| r.get(0))
            .expect("count ledger after delete");
        assert_eq!(ledger_after, 2);
    }

    // ── Test 4: idempotent — second run is a no-op ────────────────────────────

    #[tokio::test]
    async fn test_idempotent_second_run_is_noop() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        seed_old_db(&db_path);

        let m = CostLedgerCascade;
        assert!(m.run(tmp.path()).await.expect("first run"));
        assert!(
            !m.run(tmp.path()).await.expect("second run"),
            "second run must be a no-op"
        );

        // Backfill did not duplicate rows.
        let conn = Connection::open(&db_path).expect("open");
        let ledger_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM cost_ledger", [], |r| r.get(0))
            .expect("count ledger");
        assert_eq!(ledger_count, 2);
    }
}
