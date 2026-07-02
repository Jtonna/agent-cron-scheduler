//! Migration 008 — soft-delete support on the `workflows` table:
//! add the `deleted` column and replace the inline `UNIQUE`
//! constraint on `name` with a **partial unique index** over live rows.
//!
//! # Background
//!
//! `DELETE /api/workflows/{id}` switched from a hard delete (which
//! failed with a FOREIGN KEY constraint error whenever the workflow had run
//! history) to a **soft delete**. The workflow row and all of its
//! `workflow_runs` are kept — the run rows *are* the cost/token record — and
//! the workflow is simply flagged `deleted = 1`, keeping its name verbatim.
//!
//! Name uniqueness must then hold among **live** workflows only, so the old
//! inline `UNIQUE` on `workflows.name` (which SQLite cannot drop in place) is
//! removed via a standard table rebuild and replaced with:
//!
//! ```sql
//! CREATE UNIQUE INDEX idx_workflows_name_live ON workflows(name) WHERE deleted = 0;
//! ```
//!
//! # What this migration does
//!
//! If `acs.db` does not exist → return `Ok(false)` (fresh install — the new
//! table shape and index come from `schema.rs`).
//!
//! If the `workflows` table DDL no longer contains `UNIQUE` → return
//! `Ok(false)` (idempotent; already rebuilt or created fresh).
//!
//! Otherwise (standard SQLite rebuild, in one transaction with
//! `foreign_keys = OFF` for its duration):
//!
//! 1. Additively `ALTER TABLE` any columns the old table may lack
//!    (`is_favorited`, `deleted`) so the copy below is column-stable.
//! 2. `CREATE TABLE workflows_new (...)` — same shape, **no** inline `UNIQUE`.
//! 3. Copy all rows.
//! 4. `DROP TABLE workflows;` then `ALTER TABLE workflows_new RENAME TO workflows;`
//! 5. `CREATE UNIQUE INDEX idx_workflows_name_live ON workflows(name) WHERE deleted = 0;`
//! 6. `PRAGMA foreign_key_check` — abort (rollback) if the rebuild broke the
//!    `workflow_runs.workflow_id` FK.
//!
//! Existing workflows keep `deleted = 0` and their original names verbatim.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};

use crate::errors::AcsError;
use crate::migration::Migration;

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

fn run_blocking(db_path: &Path) -> Result<bool, AcsError> {
    // Open raw: do NOT run `apply_schema` here — its partial-index statement
    // must not race the rebuild, and this migration is self-contained.
    let mut conn = Connection::open(db_path)
        .map_err(|e| AcsError::Storage(format!("Failed to open SQLite at {:?}: {}", db_path, e)))?;

    // Idempotency: rebuilt (or fresh) tables have no inline UNIQUE in their DDL.
    let ddl: Option<String> = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'workflows'",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| AcsError::Storage(format!("Failed to read workflows DDL: {}", e)))?;
    let ddl = match ddl {
        // No workflows table at all — nothing to rebuild; `apply_schema` will
        // create the fresh shape at daemon startup.
        None => return Ok(false),
        Some(d) => d,
    };
    if !ddl.to_uppercase().contains("UNIQUE") {
        return Ok(false);
    }

    // The rebuild copies an explicit column list, so make sure the columns
    // added after the original CREATE TABLE exist on the old table too.
    for sql in [
        "ALTER TABLE workflows ADD COLUMN is_favorited INTEGER NOT NULL DEFAULT 0",
        "ALTER TABLE workflows ADD COLUMN deleted INTEGER NOT NULL DEFAULT 0",
    ] {
        if let Err(e) = conn.execute(sql, []) {
            if !e.to_string().contains("duplicate column name") {
                return Err(AcsError::Storage(format!(
                    "Additive column add failed ({}): {}",
                    sql, e
                )));
            }
        }
    }

    // FK enforcement must be off while the parent table is dropped/renamed.
    // PRAGMA foreign_keys is a no-op inside a transaction, so toggle outside.
    conn.execute_batch("PRAGMA foreign_keys = OFF;")
        .map_err(|e| AcsError::Storage(format!("Failed to disable foreign_keys: {}", e)))?;

    let result = rebuild_workflows_table(&mut conn);
    // Best-effort re-enable regardless of outcome; the connection is dropped
    // right after, and every production connection sets its own pragmas.
    let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
    result?;

    tracing::info!(
        "Migration m008 complete: workflows rebuilt without inline UNIQUE(name); \
         added `deleted` column and partial unique index idx_workflows_name_live"
    );
    Ok(true)
}

fn rebuild_workflows_table(conn: &mut Connection) -> Result<(), AcsError> {
    let tx = conn
        .transaction()
        .map_err(|e| AcsError::Storage(format!("Failed to start transaction: {}", e)))?;

    tx.execute_batch(
        r#"
        CREATE TABLE workflows_new (
            id                  TEXT PRIMARY KEY,
            name                TEXT NOT NULL,
            version             INTEGER NOT NULL,
            schedule            TEXT NOT NULL,
            timezone            TEXT,
            schedule_mode       TEXT NOT NULL,
            enabled             INTEGER NOT NULL,
            steps_json          TEXT NOT NULL,
            default_input       TEXT,
            working_dir         TEXT,
            env_vars            TEXT,
            allow_concurrent    INTEGER NOT NULL,
            on_failure          TEXT NOT NULL,
            last_run_at         TEXT,
            last_run_status     TEXT,
            last_run_id         TEXT,
            created_at          TEXT NOT NULL,
            updated_at          TEXT NOT NULL,
            is_favorited        INTEGER NOT NULL DEFAULT 0,
            deleted             INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO workflows_new (
            id, name, version, schedule, timezone, schedule_mode, enabled,
            steps_json, default_input, working_dir, env_vars,
            allow_concurrent, on_failure, last_run_at, last_run_status,
            last_run_id, created_at, updated_at, is_favorited, deleted
        )
        SELECT
            id, name, version, schedule, timezone, schedule_mode, enabled,
            steps_json, default_input, working_dir, env_vars,
            allow_concurrent, on_failure, last_run_at, last_run_status,
            last_run_id, created_at, updated_at, is_favorited, deleted
        FROM workflows;
        DROP TABLE workflows;
        ALTER TABLE workflows_new RENAME TO workflows;
        CREATE UNIQUE INDEX IF NOT EXISTS idx_workflows_name_live
            ON workflows(name) WHERE deleted = 0;
        "#,
    )
    .map_err(|e| AcsError::Storage(format!("workflows table rebuild failed: {}", e)))?;

    // Sanity: the rebuild must not have orphaned any workflow_runs rows.
    let violations: i64 = tx
        .query_row(
            "SELECT COUNT(*) FROM pragma_foreign_key_check('workflow_runs')",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AcsError::Storage(format!("foreign_key_check failed: {}", e)))?;
    if violations > 0 {
        return Err(AcsError::Storage(format!(
            "workflows rebuild would leave {} foreign-key violation(s); rolling back",
            violations
        )));
    }

    tx.commit()
        .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    /// Apply the pre-m008 schema (inline UNIQUE on name, no `deleted` column)
    /// to a raw connection so the migration has something to rebuild.
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

    fn insert_workflow(conn: &Connection, id: &str, name: &str) {
        conn.execute(
            "INSERT INTO workflows (id, name, version, schedule, schedule_mode, \
             enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at) \
             VALUES (?, ?, 1, '* * * * *', 'cron', 1, '[]', 1, 'abort', \
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [id, name],
        )
        .expect("insert workflow");
    }

    fn workflows_ddl(conn: &Connection) -> String {
        conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'workflows'",
            [],
            |r| r.get(0),
        )
        .expect("read DDL")
    }

    fn live_name_index_exists(conn: &Connection) -> bool {
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'index' AND name = 'idx_workflows_name_live'",
                [],
                |r| r.get(0),
            )
            .expect("query index");
        n == 1
    }

    // ── Test 1: no acs.db → no-op ────────────────────────────────────────────
    #[tokio::test]
    async fn test_no_db_file_returns_false() {
        let tmp = TempDir::new().unwrap();
        let m = AddWorkflowDeleted;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(!did_work, "missing acs.db must be a no-op");
    }

    // ── Test 2: old DB → rebuild drops inline UNIQUE, adds column + index,
    //           preserves rows and FK integrity ──────────────────────────────
    #[tokio::test]
    async fn test_rebuild_removes_unique_adds_column_and_index() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
            insert_workflow(&conn, "id-1", "legacy");
            // A child run referencing the workflow — FK must survive the rebuild.
            conn.execute(
                "INSERT INTO workflow_runs (run_id, workflow_id, workflow_version, \
                 workflow_snapshot, started_at, status, steps_json) \
                 VALUES ('run-1', 'id-1', 1, '{}', '2025-01-01T00:00:00Z', 'Completed', '[]')",
                [],
            )
            .expect("insert child run");
            assert!(
                workflows_ddl(&conn).to_uppercase().contains("UNIQUE"),
                "precondition: old table has inline UNIQUE"
            );
        }

        let m = AddWorkflowDeleted;
        let did_work = m.run(tmp.path()).await.expect("migrate");
        assert!(did_work, "migration must rebuild an old-shape table");

        let conn = Connection::open(&db_path).expect("reopen");
        // Inline UNIQUE gone; partial index present.
        assert!(
            !workflows_ddl(&conn).to_uppercase().contains("UNIQUE"),
            "inline UNIQUE must be gone after the rebuild"
        );
        assert!(
            live_name_index_exists(&conn),
            "idx_workflows_name_live must exist after the rebuild"
        );
        // Row preserved, name verbatim, deleted defaults to 0.
        let (name, deleted): (String, i64) = conn
            .query_row(
                "SELECT name, deleted FROM workflows WHERE id = 'id-1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .expect("read migrated row");
        assert_eq!(name, "legacy");
        assert_eq!(deleted, 0);
        // FK integrity intact.
        let violations: i64 = conn
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })
            .expect("fk check");
        assert_eq!(violations, 0, "rebuild must not break the workflow_runs FK");
    }

    // ── Test 3: already migrated → idempotent (Ok(false)) ────────────────────
    #[tokio::test]
    async fn test_idempotent_after_rebuild() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
        }

        let m = AddWorkflowDeleted;
        assert!(
            m.run(tmp.path()).await.expect("first run"),
            "first run rebuilds"
        );
        assert!(
            !m.run(tmp.path()).await.expect("second run"),
            "second run must be a no-op"
        );
    }

    // ── Test 4: post-migration semantics — deleted rows exempt from the
    //           name-uniqueness index, live duplicates still rejected ─────────
    #[tokio::test]
    async fn test_post_migration_name_reuse_semantics() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
            insert_workflow(&conn, "id-1", "reuse-me");
        }
        AddWorkflowDeleted.run(tmp.path()).await.expect("migrate");

        let conn = Connection::open(&db_path).expect("reopen");
        // Soft-delete the migrated row (name kept verbatim).
        conn.execute("UPDATE workflows SET deleted = 1 WHERE id = 'id-1'", [])
            .expect("soft delete");
        // A new live row with the same name must now be allowed.
        insert_workflow(&conn, "id-2", "reuse-me");
        // But a second LIVE row with that name must still conflict.
        let dup = conn.execute(
            "INSERT INTO workflows (id, name, version, schedule, schedule_mode, \
             enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at) \
             VALUES ('id-3', 'reuse-me', 1, '* * * * *', 'cron', 1, '[]', 1, 'abort', \
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        );
        assert!(dup.is_err(), "duplicate live name must be rejected");
    }

    // ── Test 5: deleted column shape (INTEGER NOT NULL DEFAULT 0) ────────────
    #[tokio::test]
    async fn test_column_type_and_default() {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("acs.db");
        {
            let conn = Connection::open(&db_path).expect("open");
            apply_old_schema(&conn);
        }
        AddWorkflowDeleted.run(tmp.path()).await.expect("migrate");

        let conn = Connection::open(&db_path).expect("reopen");
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
