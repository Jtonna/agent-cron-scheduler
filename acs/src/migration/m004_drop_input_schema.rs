//! Migration 004 — drop the `input_schema` column from the `workflows` table.
//!
//! `input_schema` is no longer carried on the `Workflow` / `NewWorkflow` /
//! `WorkflowUpdate` structs and is not consumed by the runtime. This
//! migration removes the column from the SQLite `workflows` table so that
//! INSERT/UPDATE statements that no longer reference it succeed.
//!
//! # Idempotency
//!
//! Returns `Ok(true)` when the column was present and dropped. Returns
//! `Ok(false)` when the column is absent (already migrated, or a fresh
//! install whose schema was created without the column).
//!
//! # Fresh installs
//!
//! On a fresh install `acs.db` may not exist yet — this migration runs after
//! `m002_json_to_sqlite` in the registry. If the file is missing here it
//! means there is no schema to alter; we return `Ok(false)`.

use std::path::Path;

use async_trait::async_trait;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::storage::sqlite;

const INPUT_SCHEMA_COLUMN: &str = "input_schema";

pub struct DropInputSchema;

#[async_trait]
impl Migration for DropInputSchema {
    fn name(&self) -> &'static str {
        "m004_drop_input_schema"
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

fn run_blocking(db_path: &Path) -> Result<bool, AcsError> {
    let conn = sqlite::open_with_schema(db_path)?;

    if !column_exists(&conn, "workflows", INPUT_SCHEMA_COLUMN)? {
        return Ok(false);
    }

    conn.execute("ALTER TABLE workflows DROP COLUMN input_schema", [])
        .map_err(|e| {
            AcsError::Storage(format!(
                "ALTER TABLE workflows DROP COLUMN input_schema failed: {}",
                e
            ))
        })?;

    tracing::info!("Migration m004 complete: dropped workflows.input_schema column");
    Ok(true)
}

/// Return true iff `table` has a column named `column`.
fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> Result<bool, AcsError> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .map_err(|e| AcsError::Storage(format!("PRAGMA table_info prepare failed: {}", e)))?;
    let rows = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .map_err(|e| AcsError::Storage(format!("PRAGMA table_info query failed: {}", e)))?;
    for r in rows {
        let name = r.map_err(|e| AcsError::Storage(format!("row read failed: {}", e)))?;
        if name == column {
            return Ok(true);
        }
    }
    Ok(false)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use tempfile::TempDir;
    use uuid::Uuid;

    /// Create an acs.db whose `workflows` table still has an `input_schema`
    /// column (matching the pre-m004 schema). We do this by opening with the
    /// current schema (which no longer has the column after the source-side
    /// removal in this slice) and then re-adding the column via ALTER TABLE
    /// — this simulates an existing user DB that was created before m004 ran.
    fn make_db_with_input_schema(db_path: &Path) {
        let conn = sqlite::open_with_schema(db_path).expect("open");
        // If the column happens to already be present (e.g. running this
        // test against an older copy of the schema source), do nothing.
        // Otherwise re-add it so the test can exercise the drop path.
        let exists = column_exists(&conn, "workflows", INPUT_SCHEMA_COLUMN).expect("pragma");
        if !exists {
            conn.execute("ALTER TABLE workflows ADD COLUMN input_schema TEXT", [])
                .expect("re-add input_schema");
        }
    }

    fn insert_workflow_with_input_schema(conn: &rusqlite::Connection, id: Uuid) {
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO workflows (
                id, name, version, schedule, timezone, schedule_mode, enabled,
                steps_json, input_schema, default_input, working_dir, env_vars,
                allow_concurrent, on_failure, last_run_at, last_run_status,
                last_run_id, created_at, updated_at
            ) VALUES (
                ?1, ?2, 1, '* * * * *', NULL, 'Cron', 1,
                '[]', '{\"type\":\"object\"}', NULL, NULL, NULL,
                1, '\"abort\"', NULL, NULL,
                NULL, ?3, ?3
            )",
            params![id.to_string(), format!("wf-{}", id), now],
        )
        .expect("insert workflow");
    }

    // ── Drops the column when present ───────────────────────────────────────
    #[tokio::test]
    async fn test_drops_input_schema_column_when_present() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        make_db_with_input_schema(&db_path);
        {
            let conn = sqlite::open_with_schema(&db_path).expect("open");
            insert_workflow_with_input_schema(&conn, Uuid::now_v7());
        }
        let m = DropInputSchema;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "expected Ok(true) when the column existed");

        let conn = sqlite::open_with_schema(&db_path).expect("open");
        let still_there = column_exists(&conn, "workflows", INPUT_SCHEMA_COLUMN).expect("pragma");
        assert!(
            !still_there,
            "input_schema column must be absent after migration"
        );
    }

    // ── Idempotent: absent column is a no-op ────────────────────────────────
    #[tokio::test]
    async fn test_absent_column_is_noop() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        // Open with schema → the current schema does NOT include input_schema
        // after this slice's storage changes, so the column should be absent.
        let _ = sqlite::open_with_schema(&db_path).expect("open");

        let m = DropInputSchema;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(
            !did_work,
            "expected Ok(false) when the column was already absent"
        );
    }

    // ── Fresh install (no acs.db) is a no-op ────────────────────────────────
    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = DropInputSchema;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db is a no-op");
    }
}
