//! Migration 007 — add `total_input_tokens` and `total_output_tokens` columns
//! to the `workflow_runs` table.
//!
//! # Background
//!
//! v4.2.11 makes token counts first-class alongside USD cost. Each completed
//! run now records how many input and output tokens the agent consumed, summed
//! across all agent steps in that run.
//!
//! # What this migration does
//!
//! If `acs.db` does not exist → return `Ok(false)` (fresh install — the columns
//! will be present from the start via `schema.rs`).
//!
//! If the `total_input_tokens` column already exists in `workflow_runs` →
//! return `Ok(false)` (idempotent).
//!
//! Otherwise, inside a single transaction:
//! ```sql
//! ALTER TABLE workflow_runs ADD COLUMN total_input_tokens  INTEGER NOT NULL DEFAULT 0;
//! ALTER TABLE workflow_runs ADD COLUMN total_output_tokens INTEGER NOT NULL DEFAULT 0;
//! ```
//!
//! Historical rows (written before v4.2.11) will carry `0` for both fields,
//! which is the correct sentinel for "tokens not tracked".

use std::path::Path;

use async_trait::async_trait;
use rusqlite::Connection;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

pub struct AddTokenColumns;

#[async_trait]
impl Migration for AddTokenColumns {
    fn name(&self) -> &'static str {
        "m007_add_token_columns"
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
    let mut conn = sqlite::open_with_schema(db_path)?;

    // Idempotency check: if the column already exists, nothing to do.
    if column_exists(&conn, "workflow_runs", "total_input_tokens")? {
        return Ok(false);
    }

    let tx = conn
        .transaction()
        .map_err(|e| AcsError::Storage(format!("Failed to start transaction: {}", e)))?;

    tx.execute_batch(
        "ALTER TABLE workflow_runs ADD COLUMN total_input_tokens  INTEGER NOT NULL DEFAULT 0;
         ALTER TABLE workflow_runs ADD COLUMN total_output_tokens INTEGER NOT NULL DEFAULT 0;",
    )
    .map_err(|e| AcsError::Storage(format!("ALTER TABLE failed: {}", e)))?;

    tx.commit()
        .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;

    tracing::info!(
        "Migration m007 complete: added total_input_tokens and total_output_tokens \
         columns to workflow_runs"
    );
    Ok(true)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn open_db(path: &Path) -> Connection {
        sqlite::open_with_schema(path).expect("open db")
    }

    /// Helper: check that a column exists and has the correct SQLite type,
    /// NOT NULL constraint, and DEFAULT value via PRAGMA table_info.
    fn check_column(
        conn: &Connection,
        table: &str,
        col: &str,
        expected_type: &str,
        expected_notnull: i64,
        expected_default: &str,
    ) {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({})", table))
            .expect("prepare PRAGMA");

        let rows: Vec<(String, String, i64, String)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,                    // name
                    row.get::<_, String>(2)?,                    // type
                    row.get::<_, i64>(3)?,                       // notnull
                    row.get::<_, String>(4).unwrap_or_default(), // dflt_value
                ))
            })
            .expect("query PRAGMA")
            .filter_map(|r| r.ok())
            .collect();

        let found = rows.iter().find(|(name, _, _, _)| name == col);
        assert!(
            found.is_some(),
            "column '{}' not found in table '{}'",
            col,
            table
        );
        let (_, typ, notnull, dflt) = found.unwrap();
        assert_eq!(
            typ.to_uppercase(),
            expected_type.to_uppercase(),
            "column '{}' type mismatch",
            col
        );
        assert_eq!(
            *notnull, expected_notnull,
            "column '{}' notnull mismatch",
            col
        );
        assert_eq!(*dflt, expected_default, "column '{}' default mismatch", col);
    }

    // ── Test 1: empty DB (no acs.db) → no-op ─────────────────────────────────

    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = AddTokenColumns;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db must be a no-op");
    }

    // ── Test 2: fresh DB (columns absent) → adds both columns ────────────────

    #[tokio::test]
    async fn test_adds_columns_on_fresh_db() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");

        // Create a DB that has the base schema but NOT the token columns.
        // We achieve this by opening with the schema (which in the current
        // codebase does NOT yet include the token columns) and then verifying
        // that the migration adds them.
        //
        // Since schema.rs will include the columns once this code ships, we
        // simulate a pre-migration DB by opening a fresh connection, applying
        // the base schema, and then dropping the columns via a workaround:
        // SQLite does not support DROP COLUMN in older versions, so instead
        // we open the DB without our helper (raw Connection::open) and execute
        // only the old schema DDL.
        {
            let conn = Connection::open(&db_path).expect("open");
            // Apply minimal old schema (no token columns).
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA foreign_keys = ON;
                 PRAGMA synchronous = NORMAL;
                 CREATE TABLE IF NOT EXISTS workflows (
                     id TEXT PRIMARY KEY,
                     name TEXT NOT NULL UNIQUE,
                     version INTEGER NOT NULL,
                     schedule TEXT NOT NULL,
                     timezone TEXT,
                     schedule_mode TEXT NOT NULL,
                     enabled INTEGER NOT NULL,
                     steps_json TEXT NOT NULL,
                     default_input TEXT,
                     working_dir TEXT,
                     env_vars TEXT,
                     allow_concurrent INTEGER NOT NULL,
                     on_failure TEXT NOT NULL,
                     last_run_at TEXT,
                     last_run_status TEXT,
                     last_run_id TEXT,
                     created_at TEXT NOT NULL,
                     updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS workflow_runs (
                     run_id TEXT PRIMARY KEY,
                     workflow_id TEXT NOT NULL,
                     workflow_version INTEGER NOT NULL,
                     workflow_snapshot TEXT NOT NULL,
                     started_at TEXT NOT NULL,
                     finished_at TEXT,
                     status TEXT NOT NULL,
                     trigger_input TEXT,
                     steps_json TEXT NOT NULL,
                     total_cost_usd REAL,
                     total_duration_ms INTEGER,
                     FOREIGN KEY (workflow_id) REFERENCES workflows(id)
                 );
                 CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .expect("apply old schema");
            // Confirm columns are NOT present yet.
            assert!(
                !column_exists(&conn, "workflow_runs", "total_input_tokens").unwrap(),
                "column must not exist before migration"
            );
        }

        let m = AddTokenColumns;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            did_work,
            "migration must do work on a DB without the columns"
        );

        let conn = open_db(&db_path);
        assert!(
            column_exists(&conn, "workflow_runs", "total_input_tokens").unwrap(),
            "total_input_tokens must exist after migration"
        );
        assert!(
            column_exists(&conn, "workflow_runs", "total_output_tokens").unwrap(),
            "total_output_tokens must exist after migration"
        );
    }

    // ── Test 3: already migrated → idempotent (returns Ok(false)) ────────────

    #[tokio::test]
    async fn test_idempotent_if_columns_already_present() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");

        // Create the old schema and run the migration once.
        {
            let conn = Connection::open(&db_path).expect("open");
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
                     created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS workflow_runs (
                     run_id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL,
                     workflow_version INTEGER NOT NULL, workflow_snapshot TEXT NOT NULL,
                     started_at TEXT NOT NULL, finished_at TEXT,
                     status TEXT NOT NULL, trigger_input TEXT,
                     steps_json TEXT NOT NULL, total_cost_usd REAL,
                     total_duration_ms INTEGER,
                     FOREIGN KEY (workflow_id) REFERENCES workflows(id)
                 );
                 CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .expect("apply old schema");
        }

        let m = AddTokenColumns;
        let first = m.run(tmp.path()).await.expect("first run");
        assert!(first, "first run must add the columns");

        let second = m.run(tmp.path()).await.expect("second run");
        assert!(!second, "second run must be a no-op");
    }

    // ── Test 4: column type + NOT NULL + DEFAULT 0 ────────────────────────────

    #[tokio::test]
    async fn test_columns_have_correct_type_and_default() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");

        {
            let conn = Connection::open(&db_path).expect("open");
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
                     created_at TEXT NOT NULL, updated_at TEXT NOT NULL
                 );
                 CREATE TABLE IF NOT EXISTS workflow_runs (
                     run_id TEXT PRIMARY KEY, workflow_id TEXT NOT NULL,
                     workflow_version INTEGER NOT NULL, workflow_snapshot TEXT NOT NULL,
                     started_at TEXT NOT NULL, finished_at TEXT,
                     status TEXT NOT NULL, trigger_input TEXT,
                     steps_json TEXT NOT NULL, total_cost_usd REAL,
                     total_duration_ms INTEGER,
                     FOREIGN KEY (workflow_id) REFERENCES workflows(id)
                 );
                 CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
            )
            .expect("apply old schema");
        }

        let m = AddTokenColumns;
        m.run(tmp.path()).await.expect("migrate");

        let conn = open_db(&db_path);

        // type=INTEGER, notnull=1, dflt_value=0
        check_column(
            &conn,
            "workflow_runs",
            "total_input_tokens",
            "INTEGER",
            1,
            "0",
        );
        check_column(
            &conn,
            "workflow_runs",
            "total_output_tokens",
            "INTEGER",
            1,
            "0",
        );
    }
}
