//! Flyway-style SQLite migration runner for the agent-cron-scheduler daemon.
//!
//! # Two migration kinds, one set of rules
//!
//! The registry holds two kinds of migration under a single name ordering,
//! a single `schema_migrations` tracking table, and identical execution
//! semantics:
//!
//! - **SQL migrations** — embedded named `.sql` files (via `include_str!`),
//!   executed as a script. This is the default and the documented convention
//!   for all future migrations. Scripts must NOT contain `BEGIN`/`COMMIT`;
//!   the runner owns the transaction.
//! - **Code migrations** — Rust logic implementing
//!   `fn(&rusqlite::Transaction) -> Result<(), MigrateError>`: it issues SQL
//!   against the provided transaction, reads responses, transforms in Rust,
//!   and writes back via SQL. This is the escape hatch for transforms SQL
//!   cannot express (shell tokenizing, recursion into nested JSON) — used
//!   only when justified and documented as such. It runs INSIDE the runner's
//!   per-migration transaction and is recorded/tracked exactly like a SQL
//!   migration.
//!
//! # Tracking table
//!
//! Execution is tracked by name in a `schema_migrations` table inside
//! `<data_dir>/acs.db`, created by the runner itself before any migration
//! logic runs:
//!
//! ```sql
//! CREATE TABLE schema_migrations (
//!     name        TEXT PRIMARY KEY,
//!     applied_at  TEXT NOT NULL,
//!     status      TEXT NOT NULL CHECK (status IN ('success','failed')),
//!     duration_ms INTEGER,
//!     error       TEXT
//! );
//! ```
//!
//! # Runner semantics
//!
//! [`run_pending`] walks the registry in name order. The tracking table —
//! and only the tracking table — decides what executes:
//!
//! - **no row** → run the migration inside its own transaction, then record
//!   `success` or `failed`;
//! - **`success` row** → skip without executing;
//! - **`failed` row** (for any registry migration, detected before anything
//!   runs) → record nothing, return an error naming the migration and the
//!   exact recovery statement. The daemon treats this as fatal and exits.
//!   The sanctioned recovery workflow is: fix the underlying issue, then
//!   delete the row (`DELETE FROM schema_migrations WHERE name = '<name>';`)
//!   so the next startup re-runs it.
//!
//! Each migration executes inside its own transaction. Any failure rolls the
//! migration back completely, records a `failed` row with the error text,
//! and aborts the run — later migrations never execute after a failure and
//! get no row.
//!
//! Rows recorded for names the registry does not know (e.g. `m001`/`m002`
//! from pre-v5 installs) are tolerated: reported and logged at info level,
//! never an error.
//!
//! # Rebuild convention (PRAGMA foreign_keys)
//!
//! `PRAGMA foreign_keys` is a no-op inside a transaction, so table rebuilds
//! (drop/rename of a parent table) cannot toggle it themselves. A migration
//! flagged `rebuild` gets this treatment from the runner instead:
//!
//! 1. `PRAGMA foreign_keys = OFF` before the transaction opens;
//! 2. the migration body runs inside the transaction;
//! 3. `PRAGMA foreign_key_check` runs inside the transaction before commit —
//!    any violation fails (and therefore rolls back) the migration;
//! 4. `PRAGMA foreign_keys = ON` is restored after commit or rollback.
//!
//! # Baseline convention (fresh install vs. existing database)
//!
//! A migration flagged `baseline` creates the schema starting point for
//! fresh installs. On a database that already has migration tracking (the
//! `schema_migrations` table pre-exists — every v4.2.14 install), the runner
//! records a `success` row for it WITHOUT executing, so the baseline never
//! runs against an existing schema. Databases with a schema but no tracking
//! table predate v4.2.14 and are rejected with an "upgrade through v4.2.14
//! first" error before anything runs.
//!
//! # Adding a new migration
//!
//! 1. Create `src/sql/mNNN_<name>.sql` (increment NNN past the last entry).
//! 2. Append `Migration::sql("mNNN_<name>", include_str!("sql/mNNN_<name>.sql"))`
//!    to [`registry`] — names must stay in ascending order.
//! 3. Only if the transform is impossible in SQL: add a code migration
//!    module and register it with [`Migration::code`], documenting why.

mod m005_shell_claude_to_agent;
mod m006_agent_step_normalize;
mod shell_tokens;

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use rusqlite::{Connection, Transaction};

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors surfaced by the migration runner. All of them are fatal to daemon
/// startup.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// Infrastructure failure (open/prepare/read/write on the database).
    #[error("{0}")]
    Db(String),

    /// A migration executed and failed. Its `failed` row has been recorded;
    /// later migrations were not run.
    #[error("{0}")]
    MigrationFailed(String),

    /// A previously-recorded `failed` row is blocking startup. Nothing was
    /// executed or recorded.
    #[error("{0}")]
    Blocked(String),

    /// The database has a schema but no migration tracking table — it
    /// predates v4.2.14 and must be upgraded through v4.2.14 first.
    #[error("{0}")]
    UnsupportedUpgrade(String),
}

// ── Migration type ────────────────────────────────────────────────────────────

/// A code migration body: Rust logic operating on the runner's transaction.
pub type CodeMigrationFn = Box<dyn Fn(&Transaction<'_>) -> Result<(), MigrateError> + Send + Sync>;

/// What a migration executes.
enum MigrationAction {
    /// An embedded `.sql` script, executed as a batch. Must not contain
    /// BEGIN/COMMIT — the runner owns the transaction.
    Sql(&'static str),
    /// Rust logic run inside the runner's transaction.
    Code(CodeMigrationFn),
}

/// A single forward migration step. The name is the primary key in the
/// `schema_migrations` tracking table and must never change after it ships.
pub struct Migration {
    name: &'static str,
    action: MigrationAction,
    /// Rebuild convention: toggle `PRAGMA foreign_keys` off around the
    /// transaction and run `PRAGMA foreign_key_check` before commit.
    rebuild: bool,
    /// Baseline convention: on a database that already has migration
    /// tracking, record success without executing.
    baseline: bool,
}

impl Migration {
    /// A plain SQL migration (the default kind).
    pub fn sql(name: &'static str, sql: &'static str) -> Self {
        Self {
            name,
            action: MigrationAction::Sql(sql),
            rebuild: false,
            baseline: false,
        }
    }

    /// A SQL migration that rebuilds a table participating in a foreign key
    /// (see the module docs for the rebuild convention).
    pub fn sql_rebuild(name: &'static str, sql: &'static str) -> Self {
        Self {
            name,
            action: MigrationAction::Sql(sql),
            rebuild: true,
            baseline: false,
        }
    }

    /// The fresh-install baseline SQL migration (see the module docs for the
    /// baseline convention).
    pub fn sql_baseline(name: &'static str, sql: &'static str) -> Self {
        Self {
            name,
            action: MigrationAction::Sql(sql),
            rebuild: false,
            baseline: true,
        }
    }

    /// A code migration — Rust logic for transforms SQL cannot express.
    pub fn code<F>(name: &'static str, f: F) -> Self
    where
        F: Fn(&Transaction<'_>) -> Result<(), MigrateError> + Send + Sync + 'static,
    {
        Self {
            name,
            action: MigrationAction::Code(Box::new(f)),
            rebuild: false,
            baseline: false,
        }
    }

    /// The stable migration name.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// All available migrations, ordered by name (which is also execution
/// order). Append new migrations at the end with a higher number.
pub fn registry() -> Vec<Migration> {
    vec![
        Migration::sql_baseline("m000_baseline", include_str!("sql/m000_baseline.sql")),
        Migration::sql(
            "m003_drop_step_output_summary",
            include_str!("sql/m003_drop_step_output_summary.sql"),
        ),
        Migration::sql(
            "m004_drop_input_schema",
            include_str!("sql/m004_drop_input_schema.sql"),
        ),
        Migration::code("m005_shell_claude_to_agent", m005_shell_claude_to_agent::up),
        Migration::code("m006_agent_step_normalize", m006_agent_step_normalize::up),
        Migration::sql(
            "m007_add_token_columns",
            include_str!("sql/m007_add_token_columns.sql"),
        ),
        Migration::sql_rebuild(
            "m008_add_workflow_deleted",
            include_str!("sql/m008_add_workflow_deleted.sql"),
        ),
    ]
}

// ── Run report ────────────────────────────────────────────────────────────────

/// Summary returned by [`run_pending`].
#[derive(Debug, Default)]
pub struct MigrationRunReport {
    /// Baseline migrations recorded as `success` without executing because
    /// the database already had migration tracking.
    pub seeded: Vec<String>,
    /// Skipped because the tracking table already records them as `success`.
    pub skipped: Vec<String>,
    /// Executed and recorded as `success`.
    pub ran: Vec<String>,
    /// Recorded rows whose names the registry does not know (e.g. retired
    /// pre-v5 migrations). Tolerated, never an error.
    pub unknown: Vec<String>,
}

// ── Tracking table ────────────────────────────────────────────────────────────

const TRACKING_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    name        TEXT PRIMARY KEY,
    applied_at  TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,
    error       TEXT
)";

/// Return `true` when `table` exists in the database at `db_path`, without
/// creating the file. A missing file means no tables.
fn table_exists(db_path: &Path, table: &str) -> Result<bool, MigrateError> {
    if !db_path.exists() {
        return Ok(false);
    }
    let conn = Connection::open(db_path)
        .map_err(|e| MigrateError::Db(format!("Failed to open SQLite at {:?}: {}", db_path, e)))?;
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            [table],
            |r| r.get(0),
        )
        .map_err(|e| MigrateError::Db(format!("Failed to inspect sqlite_master: {}", e)))?;
    Ok(n > 0)
}

/// Record (or overwrite) a migration's tracking row.
fn record(
    conn: &Connection,
    name: &str,
    status: &str,
    duration_ms: Option<i64>,
    error: Option<&str>,
) -> Result<(), MigrateError> {
    conn.execute(
        "INSERT OR REPLACE INTO schema_migrations \
         (name, applied_at, status, duration_ms, error) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![
            name,
            chrono::Utc::now().to_rfc3339(),
            status,
            duration_ms,
            error
        ],
    )
    .map_err(|e| MigrateError::Db(format!("Failed to write schema_migrations: {}", e)))?;
    Ok(())
}

// ── Public runner ─────────────────────────────────────────────────────────────

/// Run all pending migrations in order against `<data_dir>/acs.db`, tracked
/// by the `schema_migrations` table (see the module documentation for the
/// exact semantics). Synchronous — callers on an async runtime should wrap
/// this in a blocking task.
pub fn run_pending(data_dir: &Path) -> Result<MigrationRunReport, MigrateError> {
    run_with_registry(data_dir, &registry())
}

/// Runner core, parameterised over the registry so tests can drive it with
/// scripted migrations.
fn run_with_registry(
    data_dir: &Path,
    migrations: &[Migration],
) -> Result<MigrationRunReport, MigrateError> {
    let db_path = data_dir.join("acs.db");
    let mut report = MigrationRunReport::default();

    // Detect the database's provenance BEFORE creating anything.
    let tracking_present = table_exists(&db_path, "schema_migrations")?;
    if !tracking_present && table_exists(&db_path, "workflows")? {
        return Err(MigrateError::UnsupportedUpgrade(format!(
            "database {} has a schema but no schema_migrations tracking table, which means \
             it predates v4.2.14. Direct upgrades are supported from v4.2.14 only: install \
             v4.2.14, run it once so it records its migration state, then upgrade to v5.",
            db_path.display()
        )));
    }

    let mut conn = Connection::open(&db_path)
        .map_err(|e| MigrateError::Db(format!("Failed to open SQLite at {:?}: {}", db_path, e)))?;
    apply_runner_pragmas(&conn)?;
    conn.execute(TRACKING_TABLE_SQL, [])
        .map_err(|e| MigrateError::Db(format!("Failed to create schema_migrations: {}", e)))?;

    // Snapshot the recorded state once.
    let recorded: HashMap<String, String> = {
        let mut stmt = conn
            .prepare("SELECT name, status FROM schema_migrations")
            .map_err(|e| MigrateError::Db(format!("Failed to read schema_migrations: {}", e)))?;
        let rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| MigrateError::Db(format!("Failed to read schema_migrations: {}", e)))?;
        let mut map = HashMap::new();
        for row in rows {
            let (name, status) =
                row.map_err(|e| MigrateError::Db(format!("Row read failed: {}", e)))?;
            map.insert(name, status);
        }
        map
    };

    // Recorded rows the registry does not know about (retired pre-v5
    // migrations such as m001/m002) are tolerated and reported.
    let known: Vec<&str> = migrations.iter().map(|m| m.name).collect();
    report.unknown = recorded
        .keys()
        .filter(|name| !known.contains(&name.as_str()))
        .cloned()
        .collect();
    report.unknown.sort();
    if !report.unknown.is_empty() {
        tracing::info!(
            "schema_migrations has rows for migrations this version no longer ships \
             (tolerated, left in place): {:?}",
            report.unknown
        );
    }

    // A recorded failure blocks startup before ANYTHING runs — silently
    // continuing could compound damage on a half-migrated database.
    for migration in migrations {
        if recorded.get(migration.name).map(String::as_str) == Some("failed") {
            return Err(MigrateError::Blocked(format!(
                "migration '{}' previously failed and is blocking startup. Fix the \
                 underlying issue, then delete its tracking row so the next startup \
                 re-runs it: DELETE FROM schema_migrations WHERE name = '{}'; \
                 (database: {})",
                migration.name,
                migration.name,
                db_path.display()
            )));
        }
    }

    for migration in migrations {
        if recorded.contains_key(migration.name) {
            // Only `success` rows reach this point (failures returned above).
            report.skipped.push(migration.name.to_string());
            continue;
        }

        if migration.baseline && tracking_present {
            // Existing database: the schema the baseline would create is
            // already there in some (possibly newer) form. Record it as
            // applied without executing.
            record(&conn, migration.name, "success", None, None)?;
            report.seeded.push(migration.name.to_string());
            tracing::info!(
                "baseline migration '{}' recorded without executing (existing database)",
                migration.name
            );
            continue;
        }

        execute_one(&mut conn, migration, &db_path)?;
        report.ran.push(migration.name.to_string());
    }

    Ok(report)
}

/// Pragmas for the runner's connection. Matches the pragmas the daemon's
/// storage layer applies, so migrations execute under the same rules
/// (foreign keys enforced, WAL journaling) as production queries.
fn apply_runner_pragmas(conn: &Connection) -> Result<(), MigrateError> {
    conn.query_row("PRAGMA journal_mode = WAL;", [], |_row| Ok(()))
        .map_err(|e| MigrateError::Db(format!("Failed to set journal_mode=WAL: {}", e)))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
        .map_err(|e| MigrateError::Db(format!("Failed to apply pragmas: {}", e)))?;
    Ok(())
}

/// Execute one migration inside its own transaction, applying the rebuild
/// convention when flagged, and record the outcome row.
fn execute_one(
    conn: &mut Connection,
    migration: &Migration,
    db_path: &Path,
) -> Result<(), MigrateError> {
    let started = Instant::now();

    // PRAGMA foreign_keys is a no-op inside a transaction, so rebuilds
    // toggle it outside the transaction boundaries.
    if migration.rebuild {
        conn.execute_batch("PRAGMA foreign_keys = OFF;")
            .map_err(|e| MigrateError::Db(format!("Failed to disable foreign_keys: {}", e)))?;
    }

    let result = run_in_transaction(conn, migration);

    if migration.rebuild {
        // Best-effort restore regardless of outcome; every production
        // connection also sets its own pragmas.
        let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
    }

    let duration_ms = started.elapsed().as_millis() as i64;
    match result {
        Ok(()) => {
            record(conn, migration.name, "success", Some(duration_ms), None)?;
            tracing::info!(
                "migration '{}' applied in {}ms",
                migration.name,
                duration_ms
            );
            Ok(())
        }
        Err(e) => {
            let error_text = e.to_string();
            record(
                conn,
                migration.name,
                "failed",
                Some(duration_ms),
                Some(&error_text),
            )?;
            Err(MigrateError::MigrationFailed(format!(
                "migration '{}' failed: {}. Startup aborted; the migration was rolled back \
                 and later migrations were not run. After fixing the underlying issue, \
                 delete the tracking row so the next startup re-runs it: \
                 DELETE FROM schema_migrations WHERE name = '{}'; (database: {})",
                migration.name,
                error_text,
                migration.name,
                db_path.display()
            )))
        }
    }
}

/// The transactional body of a migration: open, run the action, verify
/// foreign keys for rebuilds, commit. Any error path drops the transaction,
/// which rolls it back.
fn run_in_transaction(conn: &mut Connection, migration: &Migration) -> Result<(), MigrateError> {
    let tx = conn
        .transaction()
        .map_err(|e| MigrateError::Db(format!("Failed to start transaction: {}", e)))?;

    match &migration.action {
        MigrationAction::Sql(sql) => tx
            .execute_batch(sql)
            .map_err(|e| MigrateError::Db(e.to_string()))?,
        MigrationAction::Code(f) => f(&tx)?,
    }

    if migration.rebuild {
        let violations: i64 = tx
            .query_row("SELECT COUNT(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })
            .map_err(|e| MigrateError::Db(format!("foreign_key_check failed: {}", e)))?;
        if violations > 0 {
            return Err(MigrateError::Db(format!(
                "rebuild would leave {} foreign-key violation(s); rolling back",
                violations
            )));
        }
    }

    tx.commit()
        .map_err(|e| MigrateError::Db(format!("COMMIT failed: {}", e)))
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn open(data_dir: &Path) -> Connection {
        Connection::open(data_dir.join("acs.db")).expect("open acs.db")
    }

    /// Read all tracking rows as (name, status, error), ordered by name.
    fn tracking_rows(data_dir: &Path) -> Vec<(String, String, Option<String>)> {
        let conn = open(data_dir);
        let mut stmt = conn
            .prepare("SELECT name, status, error FROM schema_migrations ORDER BY name")
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        rows
    }

    fn delete_row(data_dir: &Path, name: &str) {
        let conn = open(data_dir);
        conn.execute("DELETE FROM schema_migrations WHERE name = ?", [name])
            .expect("delete row");
    }

    fn insert_row(data_dir: &Path, name: &str, status: &str) {
        let conn = open(data_dir);
        conn.execute(TRACKING_TABLE_SQL, []).expect("table");
        conn.execute(
            "INSERT OR REPLACE INTO schema_migrations (name, applied_at, status) \
             VALUES (?1, '2025-01-01T00:00:00Z', ?2)",
            [name, status],
        )
        .expect("insert row");
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).expect("count query")
    }

    // ── Registry invariants ─────────────────────────────────────────────────

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

    // ── Fresh run: everything runs once, then everything skips ────────────────

    #[test]
    fn fresh_run_all_migrations_run_once_then_skip() {
        let tmp = TempDir::new().unwrap();
        let make = || {
            vec![
                Migration::sql("m001_a", "CREATE TABLE t_a (x INTEGER);"),
                Migration::sql("m002_b", "INSERT INTO t_a (x) VALUES (1);"),
            ]
        };

        let report = run_with_registry(tmp.path(), &make()).unwrap();
        assert_eq!(report.ran, vec!["m001_a", "m002_b"]);
        assert!(report.skipped.is_empty());
        assert!(report.seeded.is_empty());

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));

        // Second run: nothing executes (re-running the CREATE would fail).
        let report2 = run_with_registry(tmp.path(), &make()).unwrap();
        assert_eq!(report2.skipped, vec!["m001_a", "m002_b"]);
        assert!(report2.ran.is_empty());
        assert_eq!(
            count(&open(tmp.path()), "SELECT COUNT(*) FROM t_a"),
            1,
            "the INSERT must have run exactly once"
        );
    }

    // ── Ordering ───────────────────────────────────────────────────────────────

    #[test]
    fn execution_follows_registry_order() {
        let tmp = TempDir::new().unwrap();
        // Each migration appends its own marker; order is proven by rowid.
        let registry = vec![
            Migration::sql(
                "m001_first",
                "CREATE TABLE trace (step TEXT); INSERT INTO trace VALUES ('first');",
            ),
            Migration::sql("m002_second", "INSERT INTO trace VALUES ('second');"),
            Migration::sql("m003_third", "INSERT INTO trace VALUES ('third');"),
        ];
        run_with_registry(tmp.path(), &registry).unwrap();

        let conn = open(tmp.path());
        let mut stmt = conn
            .prepare("SELECT step FROM trace ORDER BY rowid")
            .unwrap();
        let steps: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(steps, vec!["first", "second", "third"]);
    }

    // ── Code migrations tracked identically to SQL migrations ─────────────────

    #[test]
    fn code_migration_tracked_skipped_and_rerun_identically_to_sql() {
        let tmp = TempDir::new().unwrap();
        let invocations = Arc::new(AtomicUsize::new(0));
        let make = |inv: &Arc<AtomicUsize>| {
            let inv = Arc::clone(inv);
            vec![
                Migration::sql("m001_sql", "CREATE TABLE t_sql (x INTEGER);"),
                Migration::code("m002_code", move |tx| {
                    inv.fetch_add(1, Ordering::SeqCst);
                    tx.execute_batch("CREATE TABLE t_code (x INTEGER);")
                        .map_err(|e| MigrateError::Db(e.to_string()))
                }),
            ]
        };

        // First run: both execute and get identical success rows.
        let report = run_with_registry(tmp.path(), &make(&invocations)).unwrap();
        assert_eq!(report.ran, vec!["m001_sql", "m002_code"]);
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
        assert_eq!(invocations.load(Ordering::SeqCst), 1);

        // Second run: both skip; the code body is not invoked.
        let report2 = run_with_registry(tmp.path(), &make(&invocations)).unwrap();
        assert_eq!(report2.skipped, vec!["m001_sql", "m002_code"]);
        assert_eq!(invocations.load(Ordering::SeqCst), 1, "no re-invocation");

        // Deleting the code migration's row re-runs it, same as SQL.
        delete_row(tmp.path(), "m002_code");
        open(tmp.path())
            .execute_batch("DROP TABLE t_code;")
            .unwrap();
        let report3 = run_with_registry(tmp.path(), &make(&invocations)).unwrap();
        assert_eq!(report3.skipped, vec!["m001_sql"]);
        assert_eq!(report3.ran, vec!["m002_code"]);
        assert_eq!(invocations.load(Ordering::SeqCst), 2);
    }

    // ── Failure semantics: rollback, failed row, abort, block, recover ────────

    #[test]
    fn sql_failure_rolls_back_records_failed_row_and_aborts() {
        let tmp = TempDir::new().unwrap();
        let make = || {
            vec![
                Migration::sql(
                    "m001_ok",
                    "CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);",
                ),
                // The INSERT succeeds, then the script hits a bad statement —
                // the whole migration must roll back, including that INSERT.
                Migration::sql(
                    "m002_fail",
                    "INSERT INTO t VALUES (2); INSERT INTO no_such_table VALUES (1);",
                ),
                Migration::sql("m003_never", "INSERT INTO t VALUES (3);"),
            ]
        };

        let err = run_with_registry(tmp.path(), &make()).expect_err("must abort on failure");
        let msg = err.to_string();
        assert!(msg.contains("m002_fail"), "error must name the migration");
        assert!(
            msg.contains("no_such_table"),
            "error must include the cause"
        );
        assert!(
            msg.contains("DELETE FROM schema_migrations"),
            "error must describe the recovery workflow"
        );

        // Rollback proof: the partial INSERT from m002_fail is gone.
        let conn = open(tmp.path());
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM t"),
            1,
            "m002_fail's partial work must be rolled back; only m001_ok's row remains"
        );
        drop(conn);

        // Failed row recorded with error text; m003_never has no row.
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 2, "m003_never must have no row");
        let failed = rows.iter().find(|(n, _, _)| n == "m002_fail").unwrap();
        assert_eq!(failed.1, "failed");
        assert!(
            failed
                .2
                .as_deref()
                .unwrap_or_default()
                .contains("no_such_table"),
            "failed row must carry the error text"
        );

        // Second startup: the failed row blocks before anything executes.
        let err2 = run_with_registry(tmp.path(), &make()).expect_err("failed row must block");
        assert!(err2.to_string().contains("m002_fail"));
        assert!(err2.to_string().contains("previously failed"));
        assert_eq!(
            count(&open(tmp.path()), "SELECT COUNT(*) FROM t"),
            1,
            "nothing may execute while a failed row blocks startup"
        );

        // Recovery: delete the failed row (and fix the issue) → re-runs and
        // the rest of the chain completes.
        open(tmp.path())
            .execute_batch("CREATE TABLE no_such_table (x INTEGER);")
            .unwrap();
        delete_row(tmp.path(), "m002_fail");
        let report = run_with_registry(tmp.path(), &make()).expect("must succeed after recovery");
        assert_eq!(report.skipped, vec!["m001_ok"]);
        assert_eq!(report.ran, vec!["m002_fail", "m003_never"]);
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
    }

    #[test]
    fn failed_row_blocks_even_when_earlier_migrations_are_pending() {
        let tmp = TempDir::new().unwrap();
        insert_row(tmp.path(), "m002_late", "failed");

        let registry = vec![
            Migration::sql("m001_early", "CREATE TABLE t_early (x INTEGER);"),
            Migration::sql("m002_late", "CREATE TABLE t_late (x INTEGER);"),
        ];
        let err = run_with_registry(tmp.path(), &registry).expect_err("must block");
        assert!(err.to_string().contains("m002_late"));

        // The pending earlier migration must NOT have run.
        let conn = open(tmp.path());
        let n = count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 't_early'",
        );
        assert_eq!(n, 0, "nothing runs while a failed row blocks startup");
    }

    #[test]
    fn code_failure_rolls_back_its_transaction() {
        let tmp = TempDir::new().unwrap();
        let registry = vec![
            Migration::sql(
                "m001_setup",
                "CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);",
            ),
            Migration::code("m002_code_fail", |tx| {
                // Real writes that must be rolled back when the code errors.
                tx.execute_batch("INSERT INTO t VALUES (2); INSERT INTO t VALUES (3);")
                    .map_err(|e| MigrateError::Db(e.to_string()))?;
                Err(MigrateError::Db("simulated code failure".to_string()))
            }),
        ];

        let err = run_with_registry(tmp.path(), &registry).expect_err("must fail");
        assert!(err.to_string().contains("simulated code failure"));

        // Rollback proof: the code migration's inserts are gone.
        assert_eq!(
            count(&open(tmp.path()), "SELECT COUNT(*) FROM t"),
            1,
            "code migration writes must be rolled back on failure"
        );
        let rows = tracking_rows(tmp.path());
        let failed = rows.iter().find(|(n, _, _)| n == "m002_code_fail").unwrap();
        assert_eq!(failed.1, "failed");
        assert!(failed
            .2
            .as_deref()
            .unwrap_or_default()
            .contains("simulated code failure"));
    }

    #[test]
    fn deleted_success_row_reruns_that_migration() {
        let tmp = TempDir::new().unwrap();
        let make = || {
            vec![
                Migration::sql("m001_a", "CREATE TABLE IF NOT EXISTS t (x INTEGER);"),
                Migration::sql("m002_b", "INSERT INTO t VALUES (1);"),
            ]
        };
        run_with_registry(tmp.path(), &make()).unwrap();
        delete_row(tmp.path(), "m002_b");

        let report = run_with_registry(tmp.path(), &make()).unwrap();
        assert_eq!(report.skipped, vec!["m001_a"]);
        assert_eq!(report.ran, vec!["m002_b"]);
        assert_eq!(
            count(&open(tmp.path()), "SELECT COUNT(*) FROM t"),
            2,
            "m002_b ran twice in total"
        );
        assert!(tracking_rows(tmp.path())
            .iter()
            .all(|(_, status, _)| status == "success"));
    }

    // ── Unknown recorded rows are tolerated ────────────────────────────────────

    #[test]
    fn unknown_recorded_rows_are_tolerated_and_reported() {
        let tmp = TempDir::new().unwrap();
        insert_row(tmp.path(), "m001_jobs_to_workflows", "success");
        insert_row(tmp.path(), "m002_json_to_sqlite", "success");

        let registry = vec![Migration::sql("m003_x", "CREATE TABLE t (x INTEGER);")];
        let report = run_with_registry(tmp.path(), &registry).expect("unknown rows tolerated");
        assert_eq!(
            report.unknown,
            vec!["m001_jobs_to_workflows", "m002_json_to_sqlite"]
        );
        assert_eq!(report.ran, vec!["m003_x"]);

        // Even a FAILED unknown row must not block (the registry no longer
        // ships that migration, so there is nothing to re-run).
        delete_row(tmp.path(), "m001_jobs_to_workflows");
        insert_row(tmp.path(), "m001_jobs_to_workflows", "failed");
        let report2 = run_with_registry(tmp.path(), &registry).expect("unknown failed tolerated");
        assert_eq!(report2.skipped, vec!["m003_x"]);
    }

    // ── Baseline convention ────────────────────────────────────────────────────

    #[test]
    fn baseline_executes_on_fresh_database() {
        let tmp = TempDir::new().unwrap();
        let registry = vec![
            Migration::sql_baseline("m000_base", "CREATE TABLE base (x INTEGER);"),
            Migration::sql("m001_next", "INSERT INTO base VALUES (1);"),
        ];
        let report = run_with_registry(tmp.path(), &registry).unwrap();
        assert_eq!(report.ran, vec!["m000_base", "m001_next"]);
        assert!(report.seeded.is_empty());
        assert_eq!(count(&open(tmp.path()), "SELECT COUNT(*) FROM base"), 1);
    }

    #[test]
    fn baseline_is_seeded_without_executing_on_tracked_database() {
        let tmp = TempDir::new().unwrap();
        // Simulate an existing tracked database (e.g. v4.2.14): tracking table
        // with recorded rows, plus its own schema.
        insert_row(tmp.path(), "m001_next", "success");
        open(tmp.path())
            .execute_batch("CREATE TABLE existing_data (x INTEGER);")
            .unwrap();

        let registry = vec![
            // Would fail if executed (duplicate table) — proving it doesn't run.
            Migration::sql_baseline("m000_base", "CREATE TABLE existing_data (x INTEGER);"),
            Migration::sql("m001_next", "INSERT INTO existing_data VALUES (1);"),
        ];
        let report = run_with_registry(tmp.path(), &registry).unwrap();
        assert_eq!(report.seeded, vec!["m000_base"]);
        assert_eq!(report.skipped, vec!["m001_next"]);
        assert!(report.ran.is_empty());

        let rows = tracking_rows(tmp.path());
        let base = rows.iter().find(|(n, _, _)| n == "m000_base").unwrap();
        assert_eq!(base.1, "success", "baseline recorded success either way");
    }

    #[test]
    fn pre_tracking_database_is_rejected_with_upgrade_guidance() {
        let tmp = TempDir::new().unwrap();
        // A pre-v4.2.14 database: has a workflows table, no tracking table.
        open(tmp.path())
            .execute_batch("CREATE TABLE workflows (id TEXT PRIMARY KEY);")
            .unwrap();

        let err = run_with_registry(tmp.path(), &registry()).expect_err("must reject");
        assert!(err.to_string().contains("v4.2.14"));
        // Nothing recorded, nothing executed.
        let conn = open(tmp.path());
        let n = count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'schema_migrations'",
        );
        assert_eq!(n, 0, "the tracking table must not be created");
    }

    // ── Rebuild convention ─────────────────────────────────────────────────────

    /// Parent/child pair with a real FK edge and data on both sides.
    fn setup_fk_pair(data_dir: &Path) {
        let conn = open(data_dir);
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE parent (id TEXT PRIMARY KEY);
             CREATE TABLE child (
                 id TEXT PRIMARY KEY,
                 parent_id TEXT NOT NULL,
                 FOREIGN KEY (parent_id) REFERENCES parent(id)
             );
             INSERT INTO parent VALUES ('p1');
             INSERT INTO child VALUES ('c1', 'p1');",
        )
        .expect("fk pair");
    }

    #[test]
    fn rebuild_migration_can_drop_and_rename_a_parent_table() {
        let tmp = TempDir::new().unwrap();
        setup_fk_pair(tmp.path());

        let registry = vec![Migration::sql_rebuild(
            "m001_rebuild",
            "CREATE TABLE parent_new (id TEXT PRIMARY KEY, extra INTEGER NOT NULL DEFAULT 0);
             INSERT INTO parent_new (id) SELECT id FROM parent;
             DROP TABLE parent;
             ALTER TABLE parent_new RENAME TO parent;",
        )];
        let report = run_with_registry(tmp.path(), &registry).expect("rebuild must succeed");
        assert_eq!(report.ran, vec!["m001_rebuild"]);

        let conn = open(tmp.path());
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM parent"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM child"), 1);
        // The FK edge survived the rebuild.
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM pragma_foreign_key_check"),
            0
        );
    }

    #[test]
    fn rebuild_with_fk_violation_rolls_back_and_records_failed() {
        let tmp = TempDir::new().unwrap();
        setup_fk_pair(tmp.path());

        // This rebuild drops the parent rows, orphaning the child — the
        // runner's foreign_key_check must catch it and roll everything back.
        let registry = vec![Migration::sql_rebuild(
            "m001_bad_rebuild",
            "CREATE TABLE parent_new (id TEXT PRIMARY KEY);
             DROP TABLE parent;
             ALTER TABLE parent_new RENAME TO parent;",
        )];
        let err = run_with_registry(tmp.path(), &registry).expect_err("violation must fail");
        assert!(err.to_string().contains("foreign-key violation"));

        // Rollback proof: the original parent (with its row) is intact.
        let conn = open(tmp.path());
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM parent"),
            1,
            "the original parent table and row must survive the rollback"
        );
        drop(conn);

        let rows = tracking_rows(tmp.path());
        let failed = rows
            .iter()
            .find(|(n, _, _)| n == "m001_bad_rebuild")
            .unwrap();
        assert_eq!(failed.1, "failed");
    }

    #[test]
    fn foreign_keys_enforcement_is_restored_after_a_rebuild() {
        let tmp = TempDir::new().unwrap();
        setup_fk_pair(tmp.path());

        // The migration AFTER the rebuild violates the FK — it must fail,
        // proving the runner re-enabled foreign_keys on its connection once
        // the rebuild committed.
        let registry = vec![
            Migration::sql_rebuild(
                "m001_rebuild",
                "CREATE TABLE parent_new (id TEXT PRIMARY KEY);
                 INSERT INTO parent_new (id) SELECT id FROM parent;
                 DROP TABLE parent;
                 ALTER TABLE parent_new RENAME TO parent;",
            ),
            Migration::sql(
                "m002_violate",
                "INSERT INTO child VALUES ('c2', 'no_such_parent');",
            ),
        ];
        let err = run_with_registry(tmp.path(), &registry)
            .expect_err("FK violation after the rebuild must fail");
        assert!(err.to_string().contains("m002_violate"));

        let rows = tracking_rows(tmp.path());
        let rebuild = rows.iter().find(|(n, _, _)| n == "m001_rebuild").unwrap();
        assert_eq!(rebuild.1, "success");
        let violate = rows.iter().find(|(n, _, _)| n == "m002_violate").unwrap();
        assert_eq!(violate.1, "failed");
        assert!(
            violate
                .2
                .as_deref()
                .unwrap_or_default()
                .to_uppercase()
                .contains("FOREIGN KEY"),
            "the failure must be the FK constraint; got {:?}",
            violate.2
        );
    }

    // ── Real registry ──────────────────────────────────────────────────────────

    #[test]
    fn real_registry_fresh_install_applies_everything_then_skips() {
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
        assert!(rows.iter().all(|(_, status, _)| status == "success"));

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
    fn real_registry_v4_database_runs_nothing() {
        let tmp = TempDir::new().unwrap();
        // Simulate a v4.2.14 database: tracking rows m001..m008 all success.
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
            insert_row(tmp.path(), name, "success");
        }
        // A stand-in for the existing v4 schema + data.
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
}
