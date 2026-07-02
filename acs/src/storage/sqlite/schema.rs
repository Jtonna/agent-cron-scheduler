//! SQLite schema for the ACS data store.
//!
//! All schema DDL is concentrated here so that future migrations have a single
//! known-good baseline to compare against.
//!
//! Pragmas applied on every connection:
//!
//! - `journal_mode = WAL` — better concurrency: readers and writers do not
//!   block each other the way they do with the default rollback journal.
//! - `foreign_keys = ON` — actually enforce the FK on
//!   `workflow_runs.workflow_id`. SQLite has FK disabled by default.
//! - `synchronous = NORMAL` — safe under WAL (writes are still durable after
//!   commit) and noticeably faster than FULL.

use rusqlite::Connection;

use crate::errors::AcsError;

/// All `CREATE TABLE` / `CREATE INDEX` statements applied on `init_db`.
///
/// Every statement uses `IF NOT EXISTS` so calling `apply_schema` repeatedly
/// is a no-op once the schema is in place.
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS workflows (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL UNIQUE,
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

CREATE TABLE IF NOT EXISTS workflow_runs (
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
    FOREIGN KEY (workflow_id) REFERENCES workflows(id)
);

CREATE INDEX IF NOT EXISTS idx_workflow_runs_workflow_id_finished_at
    ON workflow_runs(workflow_id, finished_at);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_finished_at
    ON workflow_runs(finished_at);
CREATE INDEX IF NOT EXISTS idx_workflow_runs_status
    ON workflow_runs(status);

CREATE TABLE IF NOT EXISTS meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);
"#;

/// Apply pragmas (WAL, foreign_keys, synchronous) to a freshly-opened
/// connection. Call this for every `Connection` you open — pragmas are
/// connection-scoped, not database-scoped.
pub fn apply_pragmas(conn: &Connection) -> Result<(), AcsError> {
    // journal_mode is a query pragma (returns the new mode); use query_row.
    conn.query_row("PRAGMA journal_mode = WAL;", [], |_row| Ok(()))
        .map_err(|e| AcsError::Storage(format!("Failed to set journal_mode=WAL: {}", e)))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
        .map_err(|e| AcsError::Storage(format!("Failed to apply pragmas: {}", e)))?;
    Ok(())
}

/// Create all tables and indexes if they do not already exist.
pub fn apply_schema(conn: &Connection) -> Result<(), AcsError> {
    conn.execute_batch(SCHEMA_SQL)
        .map_err(|e| AcsError::Storage(format!("Failed to apply schema: {}", e)))?;
    apply_additive_migrations(conn)?;
    Ok(())
}

/// Additive column migrations for tables that may already exist from older
/// daemon versions. SQLite does not support `ADD COLUMN IF NOT EXISTS`, so
/// each `ALTER TABLE ... ADD COLUMN` is executed individually and the
/// "duplicate column name" error is swallowed — that's the success-case for
/// databases that already have the column from a previous run or from the
/// `CREATE TABLE` above on a fresh install.
fn apply_additive_migrations(conn: &Connection) -> Result<(), AcsError> {
    let stmts = [
        // is_favorited added in v4.2.x — pin favorited workflows to the top.
        "ALTER TABLE workflows ADD COLUMN is_favorited INTEGER NOT NULL DEFAULT 0",
    ];
    for sql in stmts {
        if let Err(e) = conn.execute(sql, []) {
            let msg = e.to_string();
            if msg.contains("duplicate column name") {
                continue;
            }
            return Err(AcsError::Storage(format!(
                "Additive migration failed ({}): {}",
                sql, e
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_schema_is_idempotent() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        apply_pragmas(&conn).expect("pragmas");
        apply_schema(&conn).expect("first apply");
        apply_schema(&conn).expect("second apply must be a no-op");
    }

    #[test]
    fn test_pragmas_are_set() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        apply_pragmas(&conn).expect("pragmas");

        let fk: i64 = conn
            .query_row("PRAGMA foreign_keys;", [], |r| r.get(0))
            .expect("read foreign_keys");
        assert_eq!(fk, 1, "foreign_keys must be ON");

        // synchronous: 0=OFF, 1=NORMAL, 2=FULL, 3=EXTRA.
        let sync: i64 = conn
            .query_row("PRAGMA synchronous;", [], |r| r.get(0))
            .expect("read synchronous");
        assert_eq!(sync, 1, "synchronous must be NORMAL");
    }

    #[test]
    fn test_fresh_schema_has_deleted_column_defaulting_to_zero() {
        let conn = Connection::open_in_memory().expect("open");
        apply_pragmas(&conn).expect("pragmas");
        apply_schema(&conn).expect("schema");

        // Insert without specifying `deleted` — the DEFAULT 0 must apply.
        conn.execute(
            "INSERT INTO workflows (id, name, version, schedule, schedule_mode, \
             enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at) \
             VALUES ('id-1', 'wf', 1, '* * * * *', 'cron', 1, '[]', 1, 'abort', \
             '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')",
            [],
        )
        .expect("insert");
        let deleted: i64 = conn
            .query_row("SELECT deleted FROM workflows WHERE id = 'id-1'", [], |r| {
                r.get(0)
            })
            .expect("read deleted");
        assert_eq!(deleted, 0, "deleted must default to 0 on fresh install");
    }

    #[test]
    fn test_workflows_name_uniqueness_enforced() {
        let conn = Connection::open_in_memory().expect("open");
        apply_pragmas(&conn).expect("pragmas");
        apply_schema(&conn).expect("schema");

        // Insert two rows with the same name — second should fail.
        let stmt = "INSERT INTO workflows (id, name, version, schedule, schedule_mode, \
                    enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at) \
                    VALUES (?, ?, 1, '* * * * *', 'cron', 1, '[]', 1, 'abort', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z')";
        conn.execute(stmt, ["id-1", "dup-name"])
            .expect("first insert");
        let result = conn.execute(stmt, ["id-2", "dup-name"]);
        assert!(result.is_err(), "duplicate name must violate UNIQUE");
    }
}
