//! ACS's database migrations, run through the `milepost` framework.
//!
//! The framework is a generic library (see the `milepost` crate docs); this
//! module owns everything ACS-specific: the migration files themselves, the
//! registry, and the runner configuration — which database file to migrate,
//! how to detect an existing ACS schema (the `workflows` table), and the
//! upgrade guidance shown when a database predates migration tracking.
//!
//! # Migration inventory (name-ordered)
//!
//! | Name | What it does |
//! |---|---|
//! | `m000_baseline` | Fresh-install starting point: pre-m003-era schema (baseline hook) |
//! | `m003_drop_step_output_summary` | Strip legacy `output_summary` keys from run records |
//! | `m004_drop_input_schema` | Drop the retired `workflows.input_schema` column |
//! | `m005_shell_claude_to_agent` | Rewrite shell-wrapped claude steps as agent steps |
//! | `m006_agent_step_normalize` | Normalize legacy `command_template` into `model`/`extra_args` |
//! | `m007_add_token_columns` | Add per-run token-count columns |
//! | `m008_add_workflow_deleted` | Soft-delete column + live-name partial index (rebuild hook) |
//!
//! The v4-era `m001`/`m002` migrations are retired; their tracking rows on
//! upgraded databases are tolerated by the runner. Databases without a
//! tracking table at all predate v4.2.14 and are rejected with the guidance
//! configured below.
//!
//! # Adding a migration
//!
//! 1. Create `mNNN_<name>.rs` here (increment NNN past the last entry) with
//!    a unit struct implementing `milepost::Migration`. Keep the SQL in
//!    string constants; add Rust logic only where SQL alone cannot express
//!    the transform.
//! 2. Declare the module and append `Box::new(mNNN_<name>::YourStruct)` to
//!    [`registry`] — names must stay in ascending order.

mod m000_baseline;
mod m003_drop_step_output_summary;
mod m004_drop_input_schema;
mod m005_shell_claude_to_agent;
mod m006_agent_step_normalize;
mod m007_add_token_columns;
mod m008_add_workflow_deleted;
mod shell_tokens;

use std::path::Path;

use milepost::{MigrateError, Migration, MigrationRunReport, Runner};

/// All ACS migrations, ordered by name (which is also execution order).
fn registry() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(m000_baseline::Baseline),
        Box::new(m003_drop_step_output_summary::DropStepOutputSummary),
        Box::new(m004_drop_input_schema::DropInputSchema),
        Box::new(m005_shell_claude_to_agent::ShellClaudeToAgent),
        Box::new(m006_agent_step_normalize::AgentStepNormalize),
        Box::new(m007_add_token_columns::AddTokenColumns),
        Box::new(m008_add_workflow_deleted::AddWorkflowDeleted),
    ]
}

/// Run all pending ACS migrations against `<data_dir>/acs.db`.
///
/// Configures the `milepost` runner with ACS policy: the registry above, a
/// schema probe that recognises an ACS database by its `workflows` table,
/// and the upgrade guidance for databases that predate migration tracking.
/// Synchronous — the daemon wraps this in a blocking task.
pub fn run_pending(data_dir: &Path) -> Result<MigrationRunReport, MigrateError> {
    let db_path = data_dir.join("acs.db");
    let upgrade_guidance = format!(
        "database {} has a schema but no schema_migrations tracking table, which means \
         it predates v4.2.14. Direct upgrades are supported from v4.2.14 only: install \
         v4.2.14, run it once so it records its migration state, then upgrade to v5.",
        db_path.display()
    );
    let empty_history_guidance = format!(
        "database {} has an ACS schema and a schema_migrations table, but no recorded \
         migration history. Run v4.2.14 once so it records its migration state, or \
         manually seed schema_migrations rows for the migrations that are already \
         applied, then upgrade to v5.",
        db_path.display()
    );
    Runner::new(db_path)
        .migrations(registry())
        .schema_probe(|tx| tx.table_exists("workflows"), upgrade_guidance)
        .empty_history_error(empty_history_guidance)
        .run()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use tempfile::TempDir;

    fn open(data_dir: &Path) -> Connection {
        Connection::open(data_dir.join("acs.db")).expect("open acs.db")
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).expect("count query")
    }

    /// Read all tracking rows as (name, status), ordered by name.
    fn tracking_rows(data_dir: &Path) -> Vec<(String, String)> {
        let conn = open(data_dir);
        let mut stmt = conn
            .prepare("SELECT name, status FROM schema_migrations ORDER BY name")
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        rows
    }

    fn insert_row(data_dir: &Path, name: &str) {
        let conn = open(data_dir);
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS schema_migrations (
                 name        TEXT PRIMARY KEY,
                 applied_at  TEXT NOT NULL,
                 status      TEXT NOT NULL CHECK (status IN ('success','failed')),
                 duration_ms INTEGER,
                 error       TEXT
             )",
        )
        .expect("table");
        conn.execute(
            "INSERT OR REPLACE INTO schema_migrations (name, applied_at, status) \
             VALUES (?1, '2025-01-01T00:00:00Z', 'success')",
            [name],
        )
        .expect("insert row");
    }

    #[test]
    fn registry_names_are_unique_and_name_ordered() {
        let names: Vec<&str> = registry().iter().map(|m| m.name()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(
            names, sorted,
            "registry must be ordered by name with no duplicates"
        );
    }

    #[test]
    fn fresh_install_applies_everything_then_skips() {
        let tmp = TempDir::new().unwrap();
        let report = run_pending(tmp.path()).unwrap();

        assert!(report.seeded.is_empty());
        assert!(report.skipped.is_empty());
        assert_eq!(
            report.ran,
            vec![
                "m000_baseline",
                "m003_drop_step_output_summary",
                "m004_drop_input_schema",
                "m005_shell_claude_to_agent",
                "m006_agent_step_normalize",
                "m007_add_token_columns",
                "m008_add_workflow_deleted",
            ]
        );

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 7);
        assert!(rows.iter().all(|(_, status)| status == "success"));

        // Spot-check the final schema shape.
        let conn = open(tmp.path());
        let ddl: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'workflows'",
                [],
                |r| r.get(0),
            )
            .expect("workflows DDL");
        assert!(
            !ddl.to_uppercase().contains("UNIQUE"),
            "inline UNIQUE must be gone after m008"
        );
        assert!(ddl.contains("deleted"), "deleted column present");
        assert!(
            !ddl.contains("input_schema"),
            "input_schema dropped by m004"
        );
        let has_live_index = count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'idx_workflows_name_live'",
        );
        assert_eq!(has_live_index, 1, "partial live-name index present");
        let token_cols = count(
            &conn,
            "SELECT COUNT(*) FROM pragma_table_info('workflow_runs') \
             WHERE name IN ('total_input_tokens','total_output_tokens')",
        );
        assert_eq!(token_cols, 2, "token columns present after m007");
        drop(conn);

        // Second startup: everything skips.
        let report2 = run_pending(tmp.path()).unwrap();
        assert_eq!(report2.skipped.len(), 7);
        assert!(report2.ran.is_empty());
        assert!(report2.seeded.is_empty());
    }

    #[test]
    fn v4_database_runs_nothing() {
        let tmp = TempDir::new().unwrap();
        // Simulate a fully-migrated database from the previous release
        // line: tracking rows m001..m008 all success.
        for name in [
            "m001_jobs_to_workflows",
            "m002_json_to_sqlite",
            "m003_drop_step_output_summary",
            "m004_drop_input_schema",
            "m005_shell_claude_to_agent",
            "m006_agent_step_normalize",
            "m007_add_token_columns",
            "m008_add_workflow_deleted",
        ] {
            insert_row(tmp.path(), name);
        }
        // A stand-in for the pre-existing schema + data.
        open(tmp.path())
            .execute_batch(
                "CREATE TABLE workflows (id TEXT PRIMARY KEY, name TEXT NOT NULL);
                 INSERT INTO workflows VALUES ('id-1', 'wf');",
            )
            .unwrap();

        let report = run_pending(tmp.path()).unwrap();
        assert!(report.ran.is_empty(), "a v4.2.14 database runs NOTHING");
        assert_eq!(report.seeded, vec!["m000_baseline"]);
        assert_eq!(report.skipped.len(), 6, "m003..m008 skip via their rows");
        assert_eq!(
            report.unknown,
            vec!["m001_jobs_to_workflows", "m002_json_to_sqlite"],
            "retired migration rows are tolerated"
        );

        // Data intact.
        assert_eq!(
            count(&open(tmp.path()), "SELECT COUNT(*) FROM workflows"),
            1
        );
    }

    #[test]
    fn m008_rerun_against_migrated_schema_fails_loud_without_data_change() {
        // Reachable state: the schema already carries m008's outcome (the
        // `deleted` column) but m008's tracking row is missing — e.g. lost
        // to a crash window or manual row deletion. Re-running the rebuild
        // would silently reset every soft-deleted row to live; it must
        // fail loud instead, telling the operator to restore the row.
        let tmp = TempDir::new().unwrap();
        for name in [
            "m001_jobs_to_workflows",
            "m002_json_to_sqlite",
            "m003_drop_step_output_summary",
            "m004_drop_input_schema",
            "m005_shell_claude_to_agent",
            "m006_agent_step_normalize",
            "m007_add_token_columns",
        ] {
            insert_row(tmp.path(), name);
        }
        open(tmp.path())
            .execute_batch(
                "CREATE TABLE workflows (
                     id TEXT PRIMARY KEY, name TEXT NOT NULL,
                     deleted INTEGER NOT NULL DEFAULT 0
                 );
                 INSERT INTO workflows VALUES ('id-1', 'live-wf', 0);
                 INSERT INTO workflows VALUES ('id-2', 'deleted-wf', 1);",
            )
            .unwrap();

        let err = run_pending(tmp.path()).expect_err("m008 re-run must fail loud");
        let msg = err.to_string();
        assert!(msg.contains("m008_add_workflow_deleted"));
        assert!(
            msg.contains("already"),
            "error must say the migration was already applied, got: {}",
            msg
        );

        let conn = open(tmp.path());
        // Soft-delete state untouched.
        let deleted: i64 = conn
            .query_row("SELECT deleted FROM workflows WHERE id = 'id-2'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(deleted, 1, "soft-deleted row must NOT be resurrected");
        // Failed row recorded with the restore-the-row guidance.
        let (status, error): (String, Option<String>) = conn
            .query_row(
                "SELECT status, error FROM schema_migrations \
                 WHERE name = 'm008_add_workflow_deleted'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(status, "failed");
        assert!(
            error
                .unwrap_or_default()
                .contains("INSERT INTO schema_migrations"),
            "failed row must carry the tracking-row restore statement"
        );
    }

    #[test]
    fn tracked_schema_with_empty_history_gets_acs_guidance() {
        // Tracking table present but empty, plus an ACS schema. Must
        // reject with the ACS-specific guidance rather than seeding the
        // baseline and then looping on unrecoverable migration failures.
        let tmp = TempDir::new().unwrap();
        let conn = open(tmp.path());
        conn.execute_batch(
            "CREATE TABLE schema_migrations (
                 name        TEXT PRIMARY KEY,
                 applied_at  TEXT NOT NULL,
                 status      TEXT NOT NULL CHECK (status IN ('success','failed')),
                 duration_ms INTEGER,
                 error       TEXT
             );
             CREATE TABLE workflows (id TEXT PRIMARY KEY);",
        )
        .unwrap();
        drop(conn);

        let err = run_pending(tmp.path()).expect_err("must reject");
        let msg = err.to_string();
        assert!(
            msg.contains("no recorded migration history"),
            "got: {}",
            msg
        );
        assert!(msg.contains("v4.2.14"), "guidance must name the fix path");

        // Nothing recorded, nothing executed.
        assert_eq!(
            count(&open(tmp.path()), "SELECT COUNT(*) FROM schema_migrations"),
            0
        );
    }

    #[test]
    fn pre_tracking_database_is_rejected_with_upgrade_guidance() {
        let tmp = TempDir::new().unwrap();
        // A database that predates migration tracking: workflows table
        // present, no schema_migrations table.
        open(tmp.path())
            .execute_batch("CREATE TABLE workflows (id TEXT PRIMARY KEY);")
            .unwrap();

        let err = run_pending(tmp.path()).expect_err("must reject");
        assert!(err.to_string().contains("v4.2.14"));
        // Nothing recorded, nothing executed.
        let conn = open(tmp.path());
        let n = count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'schema_migrations'",
        );
        assert_eq!(n, 0, "the tracking table must not be created");
    }
}
