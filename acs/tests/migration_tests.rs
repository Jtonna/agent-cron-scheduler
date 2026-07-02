//! v5 migration acceptance tests.
//!
//! - Fresh install: the migration runner applies every migration and the
//!   resulting schema is structurally identical to a real v4.2.14-migrated
//!   database.
//! - v4.2.14 upgrade: a database with the v4.2.14 schema and recorded
//!   migration rows runs NOTHING, tolerates rows for retired migrations,
//!   and keeps its data intact.
//!
//! The v4.2.14 database is constructed from a frozen SQL snapshot below —
//! deliberately NOT from live schema code — so these tests keep guarding the
//! upgrade path even as the current schema evolves.

use rusqlite::Connection;
use tempfile::TempDir;

/// Frozen snapshot of the schema a fully-migrated v4.2.14 daemon leaves
/// behind (tables, indexes, and the migration tracking table). Copied
/// verbatim from the v4.2.14 release.
const V4_2_14_SCHEMA_SNAPSHOT: &str = r#"
CREATE TABLE workflows (
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

CREATE TABLE workflow_runs (
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

CREATE INDEX idx_workflow_runs_workflow_id_finished_at
    ON workflow_runs(workflow_id, finished_at);
CREATE INDEX idx_workflow_runs_finished_at
    ON workflow_runs(finished_at);
CREATE INDEX idx_workflow_runs_status
    ON workflow_runs(status);

CREATE TABLE meta (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_workflows_name_live
    ON workflows(name) WHERE deleted = 0;

CREATE TABLE schema_migrations (
    name        TEXT PRIMARY KEY,
    applied_at  TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,
    error       TEXT
);
"#;

/// The migration rows a fully-migrated v4.2.14 database carries.
const V4_2_14_MIGRATION_ROWS: [&str; 8] = [
    "m001_jobs_to_workflows",
    "m002_json_to_sqlite",
    "m003_drop_step_output_summary",
    "m004_drop_input_schema",
    "m005_shell_claude_to_agent",
    "m006_agent_step_normalize",
    "m007_add_token_columns",
    "m008_add_workflow_deleted",
];

/// Build a v4.2.14-shaped database (schema snapshot + recorded migration
/// rows + sample data) inside `data_dir`.
fn build_v4_database(data_dir: &std::path::Path) {
    let conn = Connection::open(data_dir.join("acs.db")).expect("create v4 acs.db");
    conn.execute_batch(V4_2_14_SCHEMA_SNAPSHOT)
        .expect("apply v4.2.14 snapshot");
    for name in V4_2_14_MIGRATION_ROWS {
        conn.execute(
            "INSERT INTO schema_migrations (name, applied_at, status, duration_ms, error) \
             VALUES (?1, '2025-06-01T00:00:00Z', 'success', 5, NULL)",
            [name],
        )
        .expect("insert migration row");
    }
    // Sample data that must survive the upgrade untouched.
    conn.execute_batch(
        "INSERT INTO workflows (id, name, version, schedule, schedule_mode, enabled,
             steps_json, allow_concurrent, on_failure, created_at, updated_at)
         VALUES ('wf-1', 'legacy-wf', 3, '* * * * *', 'Cron', 1,
             '[{\"id\":\"s0\",\"kind\":\"agent\",\"agent_type\":\"claude_code_cli\",\"prompt\":\"hi\",\"extra_args\":[]}]',
             1, '\"abort\"', '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');
         INSERT INTO workflow_runs (run_id, workflow_id, workflow_version, workflow_snapshot,
             started_at, finished_at, status, steps_json, total_cost_usd,
             total_input_tokens, total_output_tokens)
         VALUES ('run-1', 'wf-1', 3, '{}', '2025-01-02T00:00:00Z', '2025-01-02T00:01:00Z',
             'Completed', '[]', 0.5, 100, 20);",
    )
    .expect("insert sample data");
}

/// Structural fingerprint of a database schema: per table (sorted), the
/// exact column list (position, name, type, notnull, default, pk) and
/// foreign keys; per index (sorted, ignoring PK autoindexes), uniqueness,
/// origin, partiality, and the indexed columns. This is the normalized
/// comparison — raw sqlite_master DDL text legitimately differs between an
/// ALTER-based history and a CREATE-based one while describing the same
/// schema.
///
/// Known limitation: the PRAGMA-based view cannot see CHECK constraints,
/// column collations, index column direction (DESC), or WITHOUT ROWID.
/// None of those appear in the compared schemas today; if one is ever
/// introduced, extend the fingerprint accordingly.
fn schema_fingerprint(conn: &Connection) -> String {
    let mut out = String::new();

    let mut tables: Vec<String> = {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master WHERE type = 'table' \
                 AND name NOT LIKE 'sqlite_%' ORDER BY name",
            )
            .expect("prepare tables");
        stmt.query_map([], |r| r.get(0))
            .expect("query tables")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect tables")
    };
    tables.sort();

    for table in &tables {
        out.push_str(&format!("TABLE {}\n", table));

        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info({})", table))
            .expect("table_info");
        let cols = stmt
            .query_map([], |r| {
                Ok(format!(
                    "  COL {}:{} type={} notnull={} default={:?} pk={}",
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?.to_uppercase(),
                    r.get::<_, i64>(3)?,
                    r.get::<_, Option<String>>(4)?,
                    r.get::<_, i64>(5)?,
                ))
            })
            .expect("query table_info")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect table_info");
        for c in cols {
            out.push_str(&c);
            out.push('\n');
        }

        let mut stmt = conn
            .prepare(&format!("PRAGMA foreign_key_list({})", table))
            .expect("fk_list");
        let mut fks = stmt
            .query_map([], |r| {
                Ok(format!(
                    "  FK table={} from={} to={:?}",
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, Option<String>>(4)?,
                ))
            })
            .expect("query fk_list")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect fk_list");
        fks.sort();
        for f in fks {
            out.push_str(&f);
            out.push('\n');
        }
    }

    // Named indexes (PK/UNIQUE autoindexes are implied by the column data
    // above and carry generated names, so they are skipped).
    let mut stmt = conn
        .prepare(
            "SELECT name, tbl_name FROM sqlite_master WHERE type = 'index' \
             AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .expect("prepare indexes");
    let indexes: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .expect("query indexes")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect indexes");

    for (index, table) in indexes {
        // unique + partial flags come from index_list on the owning table.
        let mut stmt = conn
            .prepare(&format!("PRAGMA index_list({})", table))
            .expect("index_list");
        let meta = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(1)?, // name
                    r.get::<_, i64>(2)?,    // unique
                    r.get::<_, String>(3)?, // origin
                    r.get::<_, i64>(4)?,    // partial
                ))
            })
            .expect("query index_list")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect index_list")
            .into_iter()
            .find(|(n, _, _, _)| n == &index)
            .expect("index must appear in index_list");

        let mut stmt = conn
            .prepare(&format!("PRAGMA index_info({})", index))
            .expect("index_info");
        let cols: Vec<String> = stmt
            .query_map([], |r| r.get::<_, Option<String>>(2))
            .expect("query index_info")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect index_info")
            .into_iter()
            .map(|c| c.unwrap_or_else(|| "<expr>".to_string()))
            .collect();

        // The WHERE clause of a partial index only lives in the DDL text —
        // normalize whitespace and case for comparison.
        let ddl: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'index' AND name = ?1",
                [&index],
                |r| r.get(0),
            )
            .expect("index DDL");
        let where_clause = ddl
            .as_deref()
            .and_then(|sql| {
                let upper = sql.to_uppercase();
                upper.find(" WHERE ").map(|pos| {
                    sql[pos..]
                        .split_whitespace()
                        .collect::<Vec<_>>()
                        .join(" ")
                        .to_lowercase()
                })
            })
            .unwrap_or_default();

        out.push_str(&format!(
            "INDEX {} on={} unique={} origin={} partial={} cols={:?} {}\n",
            index, table, meta.1, meta.2, meta.3, cols, where_clause
        ));
    }

    out
}

// ─── A1: fresh v5 install produces the v4.2.14-identical schema ──────────────

#[test]
fn fresh_v5_schema_is_identical_to_migrated_v4_2_14_schema() {
    // Fresh v5 install: migration runner first.
    let fresh = TempDir::new().expect("tmp fresh");
    let report = agent_cron_scheduler::migrations::run_pending(fresh.path()).expect("fresh run");
    assert_eq!(report.ran.len(), 7, "all v5 migrations must run fresh");

    // v4.2.14 upgrade: snapshot database through the same runner.
    let upgraded = TempDir::new().expect("tmp v4");
    build_v4_database(upgraded.path());
    let report =
        agent_cron_scheduler::migrations::run_pending(upgraded.path()).expect("upgrade run");
    assert!(report.ran.is_empty(), "upgrade must execute nothing");

    // Compare IMMEDIATELY after the runner, before init_db: the idempotent
    // apply_schema would repair identical omissions on both sides and mask
    // gaps in the migration chain itself.
    {
        let fresh_conn = Connection::open(fresh.path().join("acs.db")).expect("open fresh");
        let upgraded_conn =
            Connection::open(upgraded.path().join("acs.db")).expect("open upgraded");
        assert_eq!(
            schema_fingerprint(&fresh_conn),
            schema_fingerprint(&upgraded_conn),
            "runner output alone must already be structurally identical to a \
             migrated v4.2.14 schema (before init_db's idempotent repair)"
        );
    }

    // Then complete the daemon startup order: the storage layer's
    // idempotent schema application.
    let _db = agent_cron_scheduler::storage::sqlite::init_db(&fresh.path().join("acs.db"))
        .expect("init_db fresh");
    let _db = agent_cron_scheduler::storage::sqlite::init_db(&upgraded.path().join("acs.db"))
        .expect("init_db upgraded");

    let fresh_conn = Connection::open(fresh.path().join("acs.db")).expect("open fresh");
    let upgraded_conn = Connection::open(upgraded.path().join("acs.db")).expect("open upgraded");

    let fresh_fp = schema_fingerprint(&fresh_conn);
    let upgraded_fp = schema_fingerprint(&upgraded_conn);
    assert_eq!(
        fresh_fp, upgraded_fp,
        "fresh v5 schema must be structurally identical to a migrated v4.2.14 schema"
    );

    // Belt and braces: the fingerprint covered the tables we expect.
    for table in ["workflows", "workflow_runs", "meta", "schema_migrations"] {
        assert!(
            fresh_fp.contains(&format!("TABLE {}", table)),
            "fingerprint must include {}",
            table
        );
    }
    assert!(fresh_fp.contains("INDEX idx_workflows_name_live"));
}

// ─── A2: v4.2.14 upgrade runs nothing and keeps data intact ──────────────────

#[test]
fn v4_2_14_upgrade_runs_nothing_and_preserves_data() {
    let tmp = TempDir::new().expect("tmp");
    build_v4_database(tmp.path());

    let report =
        agent_cron_scheduler::migrations::run_pending(tmp.path()).expect("upgrade must succeed");
    assert!(
        report.ran.is_empty(),
        "a v4.2.14 database must not execute any migration, got {:?}",
        report.ran
    );
    assert_eq!(
        report.seeded,
        vec!["m000_baseline"],
        "only the baseline row is seeded (recorded without executing)"
    );
    assert_eq!(
        report.skipped.len(),
        6,
        "m003..m008 skip via their recorded rows"
    );
    assert_eq!(
        report.unknown,
        vec!["m001_jobs_to_workflows", "m002_json_to_sqlite"],
        "retired m001/m002 rows are tolerated, never an error"
    );

    // Second startup: same result, still nothing to do.
    let report2 = agent_cron_scheduler::migrations::run_pending(tmp.path()).expect("second run");
    assert!(report2.ran.is_empty());
    assert!(report2.seeded.is_empty(), "baseline row already present");

    // Data intact, byte for byte where it matters.
    let conn = Connection::open(tmp.path().join("acs.db")).expect("open");
    let (name, version, steps_json): (String, i64, String) = conn
        .query_row(
            "SELECT name, version, steps_json FROM workflows WHERE id = 'wf-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("workflow row");
    assert_eq!(name, "legacy-wf");
    assert_eq!(version, 3, "version untouched — no migration rewrote it");
    assert!(steps_json.contains("claude_code_cli"));

    let (cost, input_tokens, output_tokens): (f64, i64, i64) = conn
        .query_row(
            "SELECT total_cost_usd, total_input_tokens, total_output_tokens \
             FROM workflow_runs WHERE run_id = 'run-1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .expect("run row");
    assert!((cost - 0.5).abs() < 1e-9);
    assert_eq!(input_tokens, 100);
    assert_eq!(output_tokens, 20);
}

// ─── A3 (integration slice): failed row blocks daemon-visible startup path ───
//
// The full failure-semantics matrix (rollback proof, failed-row recording,
// delete-to-rerun) is covered by the milepost framework's own unit tests
// and the migrations module tests; this slice
// proves the failure surface reaches callers of the public run_pending API.

#[test]
fn failed_migration_row_blocks_run_pending_until_deleted() {
    let tmp = TempDir::new().expect("tmp");
    build_v4_database(tmp.path());

    // Mark one v5-known migration as failed.
    let conn = Connection::open(tmp.path().join("acs.db")).expect("open");
    conn.execute(
        "UPDATE schema_migrations SET status = 'failed', error = 'disk full' \
         WHERE name = 'm007_add_token_columns'",
        [],
    )
    .expect("mark failed");
    drop(conn);

    let err = agent_cron_scheduler::migrations::run_pending(tmp.path())
        .expect_err("failed row must block startup");
    let msg = err.to_string();
    assert!(msg.contains("m007_add_token_columns"));
    assert!(
        msg.contains("DELETE FROM schema_migrations WHERE name = 'm007_add_token_columns'"),
        "error must carry the exact recovery statement, got: {}",
        msg
    );

    // Operator recovery: delete the row → the migration re-runs. m007 would
    // fail against a v4.2.14 table that already has the token columns, so
    // drop them first to simulate the fixed precondition.
    let conn = Connection::open(tmp.path().join("acs.db")).expect("open");
    conn.execute_batch(
        "ALTER TABLE workflow_runs DROP COLUMN total_input_tokens;
         ALTER TABLE workflow_runs DROP COLUMN total_output_tokens;
         DELETE FROM schema_migrations WHERE name = 'm007_add_token_columns';",
    )
    .expect("recover");
    drop(conn);

    let report = agent_cron_scheduler::migrations::run_pending(tmp.path())
        .expect("must succeed after recovery");
    assert_eq!(report.ran, vec!["m007_add_token_columns"]);
}
