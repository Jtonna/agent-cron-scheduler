//! Migration 008 — add the `deleted` soft-delete column to the `workflows`
//! table.
//!
//! # Background
//!
//! v4.2.14 (ACS-25) switches `DELETE /api/workflows/{id}` from a hard delete
//! (which failed with a FOREIGN KEY constraint error whenever the workflow had
//! run history) to a **soft delete**. The workflow row and all of its
//! `workflow_runs` are kept — the run rows *are* the cost/token record — and
//! the workflow is simply flagged `deleted = 1`.
//!
//! # What this migration does
//!
//! If `acs.db` does not exist → return `Ok(false)` (fresh install — the column
//! is present from the start via `schema.rs`).
//!
//! If the `deleted` column already exists in `workflows` → return `Ok(false)`
//! (idempotent).
//!
//! Otherwise:
//! ```sql
//! ALTER TABLE workflows ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0;
//! ```
//!
//! Existing workflows keep `deleted = 0` (the correct "not deleted" sentinel).
//! No table rebuild, no new tables, no foreign-key changes.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::Connection;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

pub struct AddWorkflowDeleted;

#[async_trait]
impl Migration for AddWorkflowDeleted {
    fn name(&self) -> &'static str {
        "m008_add_workflow_deleted"
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

fn column_exists(conn: &Connection, table: &str, column: &str) -> Result<bool, AcsError> {
    // PRAGMA table_info returns one row per column with fields:
    //   cid | name | type | notnull | dflt_value | pk
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| AcsError::Storage(format!("Failed to prepare PRAGMA table_info: {}", e)))?;

    let names: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|e| AcsError::Storage(format!("Failed to query PRAGMA table_info: {}", e)))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| AcsError::Storage(format!("Failed to collect column names: {}", e)))?;

    Ok(names.iter().any(|n| n == column))
}

fn run_blocking(db_path: &Path) -> Result<bool, AcsError> {
    let conn = sqlite::open_with_schema(db_path)?;

    // Idempotency check: if the column already exists, nothing to do.
    if column_exists(&conn, "workflows", "deleted")? {
        return Ok(false);
    }

    conn.execute(
        "ALTER TABLE workflows ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
        [],
    )
    .map_err(|e| AcsError::Storage(format!("ALTER TABLE failed: {}", e)))?;

    tracing::info!("Migration m008 complete: added `deleted` column to workflows");
    Ok(true)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    /// Apply the pre-m008 `workflows` schema (no `deleted` column) to a raw
    /// connection so the migration has something to migrate.
    fn apply_old_schema(conn: &Connection) {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
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
             CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
        )
        .expect("apply old schema");
    }

    // ── Test 1: no acs.db → no-op ────────────────────────────────────────────
    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = AddWorkflowDeleted;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db must be a no-op");
    }

    // ── Test 2: fresh (old) DB → adds the column, default 0 ──────────────────
    #[tokio::test]
    async fn test_adds_column_with_default_false() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
            // Insert an existing workflow to prove it survives the migration.
            conn.execute(
                "INSERT INTO workflows (id, name, version, schedule, schedule_mode, \
                 enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at) \
                 VALUES ('id-1', 'legacy', 1, '* * * * *', 'cron', 1, '[]', 1, 'abort', \
                 '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
                [],
            )
            .expect("insert legacy row");
            assert!(
                !column_exists(&conn, "workflows", "deleted").unwrap(),
                "column must not exist before migration"
            );
        }

        let m = AddWorkflowDeleted;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            did_work,
            "migration must do work on a DB without the column"
        );

        let conn = sqlite::open_with_schema(&db_path).expect("reopen");
        assert!(
            column_exists(&conn, "workflows", "deleted").unwrap(),
            "deleted column must exist after migration"
        );
        // Existing workflow must be unaffected: deleted defaults to 0.
        let deleted: i64 = conn
            .query_row("SELECT deleted FROM workflows WHERE id = 'id-1'", [], |r| {
                r.get(0)
            })
            .expect("read deleted");
        assert_eq!(deleted, 0, "existing workflow must default to deleted = 0");
    }

    // ── Test 3: already migrated → idempotent (Ok(false)) ────────────────────
    #[tokio::test]
    async fn test_idempotent_if_column_present() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
        }

        let m = AddWorkflowDeleted;
        assert!(m.run(tmp.path()).await.expect("first run"), "first adds");
        assert!(
            !m.run(tmp.path()).await.expect("second run"),
            "second run must be a no-op"
        );
    }

    // ── Test 4: column type + NOT NULL + DEFAULT 0 ───────────────────────────
    #[tokio::test]
    async fn test_column_type_and_default() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
        }
        AddWorkflowDeleted.run(tmp.path()).await.expect("migrate");

        let conn = sqlite::open_with_schema(&db_path).expect("reopen");
        let mut stmt = conn
            .prepare("PRAGMA table_info(workflows)")
            .expect("prepare PRAGMA");
        let row = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?,                    // name
                    r.get::<_, String>(2)?,                    // type
                    r.get::<_, i64>(3)?,                       // notnull
                    r.get::<_, String>(4).unwrap_or_default(), // dflt_value
                ))
            })
            .expect("query PRAGMA")
            .filter_map(|r| r.ok())
            .find(|(name, _, _, _)| name == "deleted")
            .expect("deleted column present");
        assert_eq!(row.1.to_uppercase(), "INTEGER", "type mismatch");
        assert_eq!(row.2, 1, "must be NOT NULL");
        assert_eq!(row.3, "0", "default must be 0");
    }
}
