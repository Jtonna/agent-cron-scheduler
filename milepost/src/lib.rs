//! A small, table-driven SQLite migration framework.
//!
//! Like mileposts along a road, migrations are named markers laid down in
//! order — the framework records which ones a database has passed and runs
//! the rest.
//!
//! `milepost` is a generic library: it contains only framework
//! functionality and knows nothing about the application using it. The
//! application defines its own migrations, builds a registry, and hands
//! both to the [`Runner`]:
//!
//! ```
//! use milepost::{MigrateError, Migration, MigrationTx, Runner};
//!
//! struct CreateUsers;
//!
//! impl Migration for CreateUsers {
//!     fn name(&self) -> &'static str {
//!         "m001_create_users"
//!     }
//!     fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
//!         tx.execute_batch("CREATE TABLE users (id TEXT PRIMARY KEY);")
//!     }
//! }
//!
//! let dir = tempfile::tempdir().unwrap();
//! let report = Runner::new(dir.path().join("app.db"))
//!     .migrations(vec![Box::new(CreateUsers)])
//!     .run()
//!     .unwrap();
//! assert_eq!(report.ran, vec!["m001_create_users"]);
//! ```
//!
//! # One migration kind
//!
//! Every migration is a Rust type implementing the [`Migration`] trait. The
//! framework hands the migration a [`MigrationTx`] — a small SQL-string API
//! over the runner-owned transaction: execute runnable SQL from string
//! constants, and read query output back as plain Rust values
//! ([`SqlValue`]). Simple migrations are a single `execute_batch` of a SQL
//! constant; complex migrations mix SQL strings with Rust-level logic in
//! ways plain SQL cannot express.
//!
//! # Tracking table
//!
//! Execution is tracked by name in a `schema_migrations` table inside the
//! target database (the table name is a framework convention), created by
//! the runner itself before any migration logic runs:
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
//! [`Runner::run`] walks the registry in order. The tracking table — and
//! only the tracking table — decides what executes:
//!
//! - **no row** → run the migration inside its own transaction, then record
//!   `success` or `failed`;
//! - **`success` row** → skip without executing;
//! - **`failed` row** (for any registry migration, detected before anything
//!   runs) → record nothing, return an error naming the migration and the
//!   exact recovery statement. The sanctioned recovery procedure is: fix the
//!   underlying issue, then delete the row
//!   (`DELETE FROM schema_migrations WHERE name = '<name>';`) so the next
//!   run re-runs it.
//!
//! Each migration executes inside its own transaction. Any failure rolls the
//! migration back completely, records a `failed` row with the error text,
//! and aborts the run — later migrations never execute after a failure and
//! get no row.
//!
//! Rows recorded for names the registry does not know (e.g. migrations
//! retired by the application) are tolerated: reported and logged at info
//! level, never an error.
//!
//! # Rebuild convention (PRAGMA foreign_keys)
//!
//! `PRAGMA foreign_keys` is a no-op inside a transaction, so table rebuilds
//! (drop/rename of a parent table) cannot toggle it themselves. A migration
//! whose [`Migration::rebuild`] hook returns `true` gets this treatment from
//! the runner instead:
//!
//! 1. `PRAGMA foreign_keys = OFF` before the transaction opens;
//! 2. the migration body runs inside the transaction;
//! 3. `PRAGMA foreign_key_check` runs inside the transaction before commit —
//!    any violation fails (and therefore rolls back) the migration;
//! 4. `PRAGMA foreign_keys = ON` is restored after commit or rollback.
//!
//! # Baseline convention and the schema probe
//!
//! A migration whose [`Migration::baseline`] hook returns `true` creates the
//! schema starting point for fresh installs. On a database that already has
//! both migration tracking and a schema, the runner records a `success` row
//! for it WITHOUT executing, so the baseline never runs against an existing
//! schema.
//!
//! "Has a schema" is application knowledge, so the caller supplies it via
//! [`Runner::schema_probe`]: a closure that inspects the database (e.g.
//! checks that a well-known table exists) and returns `bool`. The probe
//! serves two purposes:
//!
//! - **Crash-window protection**: if the tracking table exists but the
//!   probe reports no schema — a previous run died after the runner created
//!   the tracking table but before the baseline ever executed — the
//!   baseline executes normally instead of being seeded.
//! - **Untracked-schema guard**: if the probe reports a schema but there is
//!   no tracking table, the database predates migration tracking entirely;
//!   the runner rejects it with the caller-supplied message before creating
//!   or recording anything.
//!
//! Without a probe, the runner assumes a tracked database already has its
//! schema (baselines seed whenever the tracking table pre-exists) and the
//! untracked-schema guard is disabled.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use rusqlite::types::{ToSqlOutput, Value, ValueRef};
use rusqlite::{Connection, ToSql, Transaction};

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors surfaced by the migration runner. All of them should be treated
/// as fatal by the caller — the database is not in a state to run against.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// Infrastructure failure (open/prepare/read/write on the database).
    #[error("{0}")]
    Db(String),

    /// A migration executed and failed. Its `failed` row has been recorded;
    /// later migrations were not run.
    #[error("{0}")]
    MigrationFailed(String),

    /// A previously-recorded `failed` row is blocking the run. Nothing was
    /// executed or recorded.
    #[error("{0}")]
    Blocked(String),

    /// The schema probe found a schema but the database has no migration
    /// tracking table — it predates migration tracking and was rejected
    /// with the caller-supplied guard message.
    #[error("{0}")]
    UntrackedSchema(String),
}

// ── SQL-string API: values ────────────────────────────────────────────────────

/// A single SQLite value, as read from or bound into a SQL statement.
/// Plain Rust data — no ORM, no derive machinery.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl SqlValue {
    /// The text content, if this value is `Text`.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            SqlValue::Text(s) => Some(s),
            _ => None,
        }
    }

    /// The integer content, if this value is `Integer`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            SqlValue::Integer(i) => Some(*i),
            _ => None,
        }
    }

    /// The float content, if this value is `Real`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            SqlValue::Real(f) => Some(*f),
            _ => None,
        }
    }

    /// The blob content, if this value is `Blob`.
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            SqlValue::Blob(b) => Some(b),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, SqlValue::Null)
    }
}

impl From<&str> for SqlValue {
    fn from(s: &str) -> Self {
        SqlValue::Text(s.to_string())
    }
}

impl From<String> for SqlValue {
    fn from(s: String) -> Self {
        SqlValue::Text(s)
    }
}

impl From<i64> for SqlValue {
    fn from(i: i64) -> Self {
        SqlValue::Integer(i)
    }
}

impl From<f64> for SqlValue {
    fn from(f: f64) -> Self {
        SqlValue::Real(f)
    }
}

impl ToSql for SqlValue {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
        Ok(match self {
            SqlValue::Null => ToSqlOutput::Owned(Value::Null),
            SqlValue::Integer(i) => ToSqlOutput::Owned(Value::Integer(*i)),
            SqlValue::Real(f) => ToSqlOutput::Owned(Value::Real(*f)),
            SqlValue::Text(s) => ToSqlOutput::Borrowed(ValueRef::Text(s.as_bytes())),
            SqlValue::Blob(b) => ToSqlOutput::Borrowed(ValueRef::Blob(b)),
        })
    }
}

impl From<ValueRef<'_>> for SqlValue {
    fn from(v: ValueRef<'_>) -> Self {
        match v {
            ValueRef::Null => SqlValue::Null,
            ValueRef::Integer(i) => SqlValue::Integer(i),
            ValueRef::Real(f) => SqlValue::Real(f),
            ValueRef::Text(t) => SqlValue::Text(String::from_utf8_lossy(t).into_owned()),
            ValueRef::Blob(b) => SqlValue::Blob(b.to_vec()),
        }
    }
}

// ── SQL-string API: the runner-owned transaction handle ──────────────────────

/// The framework's SQL-string API over the runner-owned transaction.
///
/// A migration receives this in [`Migration::up`] and uses it to execute SQL
/// from string constants and to read query output back as plain Rust values.
/// Everything a migration does goes through this handle, so the whole
/// migration commits or rolls back atomically with its tracking outcome.
pub struct MigrationTx<'a> {
    tx: &'a Transaction<'a>,
}

impl<'a> MigrationTx<'a> {
    /// Wrap an existing transaction — useful for unit-testing a migration
    /// body directly against an in-memory database. In production the
    /// runner constructs this and owns the commit/rollback decision.
    pub fn new(tx: &'a Transaction<'a>) -> Self {
        Self { tx }
    }

    /// Execute one or more `;`-separated SQL statements with no parameters
    /// and no results. Must not contain `BEGIN`/`COMMIT` — the runner owns
    /// the transaction.
    pub fn execute_batch(&self, sql: &str) -> Result<(), MigrateError> {
        self.tx
            .execute_batch(sql)
            .map_err(|e| MigrateError::Db(e.to_string()))
    }

    /// Execute a single SQL statement with positional (`?1`, `?2`, …)
    /// parameters. Returns the number of rows affected.
    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize, MigrateError> {
        self.tx
            .execute(sql, rusqlite::params_from_iter(params.iter()))
            .map_err(|e| MigrateError::Db(e.to_string()))
    }

    /// Run a query with positional parameters, returning every row as a
    /// `Vec<SqlValue>` in column order.
    pub fn query(
        &self,
        sql: &str,
        params: &[SqlValue],
    ) -> Result<Vec<Vec<SqlValue>>, MigrateError> {
        let mut stmt = self
            .tx
            .prepare(sql)
            .map_err(|e| MigrateError::Db(e.to_string()))?;
        let column_count = stmt.column_count();
        let mut rows = stmt
            .query(rusqlite::params_from_iter(params.iter()))
            .map_err(|e| MigrateError::Db(e.to_string()))?;
        let mut out = Vec::new();
        while let Some(row) = rows.next().map_err(|e| MigrateError::Db(e.to_string()))? {
            let mut values = Vec::with_capacity(column_count);
            for i in 0..column_count {
                let v = row
                    .get_ref(i)
                    .map_err(|e| MigrateError::Db(e.to_string()))?;
                values.push(SqlValue::from(v));
            }
            out.push(values);
        }
        Ok(out)
    }

    /// Return `true` when a table with the given name exists. The most
    /// common building block for schema probes and in-migration guards.
    pub fn table_exists(&self, table: &str) -> Result<bool, MigrateError> {
        let rows = self.query(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            &[SqlValue::from(table)],
        )?;
        Ok(!rows.is_empty())
    }
}

// ── Migration trait ───────────────────────────────────────────────────────────

/// A single forward migration step: a Rust type whose SQL lives in string
/// constants, executed through the runner-provided [`MigrationTx`].
///
/// The name is the primary key in the `schema_migrations` tracking table and
/// must never change after it ships.
pub trait Migration: Send + Sync {
    /// The stable migration name, e.g. `"m001_create_users"`. Registries
    /// are conventionally kept in ascending name order.
    fn name(&self) -> &'static str;

    /// Baseline convention: on a database that already has migration
    /// tracking and a schema, record success without executing. See the
    /// crate docs.
    fn baseline(&self) -> bool {
        false
    }

    /// Rebuild convention: toggle `PRAGMA foreign_keys` off around the
    /// transaction and run `PRAGMA foreign_key_check` before commit. See the
    /// crate docs.
    fn rebuild(&self) -> bool {
        false
    }

    /// The migration body, run inside the runner-owned transaction.
    fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError>;
}

// ── Run report ────────────────────────────────────────────────────────────────

/// Summary returned by [`Runner::run`].
#[derive(Debug, Default)]
pub struct MigrationRunReport {
    /// Baseline migrations recorded as `success` without executing because
    /// the database already had migration tracking and a schema.
    pub seeded: Vec<String>,
    /// Skipped because the tracking table already records them as `success`.
    pub skipped: Vec<String>,
    /// Executed and recorded as `success`.
    pub ran: Vec<String>,
    /// Recorded rows whose names the registry does not know (e.g.
    /// migrations retired by the application). Tolerated, never an error.
    pub unknown: Vec<String>,
}

// ── Runner ────────────────────────────────────────────────────────────────────

/// A schema probe: application-supplied detection of whether the database
/// already carries the application's schema. See the crate docs.
pub type SchemaProbe = Box<dyn Fn(&MigrationTx<'_>) -> Result<bool, MigrateError> + Send + Sync>;

/// The migration runner: configured with a database path, a caller-supplied
/// registry, and (optionally) a schema probe, then executed with
/// [`Runner::run`].
pub struct Runner {
    db_path: PathBuf,
    migrations: Vec<Box<dyn Migration>>,
    schema_probe: Option<SchemaProbe>,
    untracked_schema_error: Option<String>,
}

impl Runner {
    /// Create a runner for the database file at `db_path`. The file (and
    /// the tracking table inside it) is created on first run; the parent
    /// directory must already exist.
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            migrations: Vec::new(),
            schema_probe: None,
            untracked_schema_error: None,
        }
    }

    /// Supply the application's migration registry, in execution order.
    pub fn migrations(mut self, migrations: Vec<Box<dyn Migration>>) -> Self {
        self.migrations = migrations;
        self
    }

    /// Supply the application's schema probe together with the error
    /// message returned when the probe finds a schema on a database that
    /// has no migration tracking table (see the crate docs for both roles
    /// the probe plays).
    pub fn schema_probe<F>(mut self, probe: F, untracked_schema_error: impl Into<String>) -> Self
    where
        F: Fn(&MigrationTx<'_>) -> Result<bool, MigrateError> + Send + Sync + 'static,
    {
        self.schema_probe = Some(Box::new(probe));
        self.untracked_schema_error = Some(untracked_schema_error.into());
        self
    }

    /// Run all pending migrations in order, tracked by the
    /// `schema_migrations` table (see the crate documentation for the exact
    /// semantics). Synchronous — callers on an async runtime should wrap
    /// this in a blocking task.
    pub fn run(&self) -> Result<MigrationRunReport, MigrateError> {
        let mut report = MigrationRunReport::default();

        // Detect the database's provenance BEFORE creating anything.
        let (tracking_present, schema_present) = self.detect()?;
        if !tracking_present && schema_present {
            return Err(MigrateError::UntrackedSchema(
                self.untracked_schema_error
                    .clone()
                    .unwrap_or_else(|| default_untracked_schema_error(&self.db_path)),
            ));
        }

        let mut conn = Connection::open(&self.db_path).map_err(|e| {
            MigrateError::Db(format!(
                "Failed to open SQLite at {:?}: {}",
                self.db_path, e
            ))
        })?;
        apply_runner_pragmas(&conn)?;
        conn.execute(TRACKING_TABLE_SQL, [])
            .map_err(|e| MigrateError::Db(format!("Failed to create schema_migrations: {}", e)))?;

        // Snapshot the recorded state once.
        let recorded: HashMap<String, String> = {
            let mut stmt = conn
                .prepare("SELECT name, status FROM schema_migrations")
                .map_err(|e| {
                    MigrateError::Db(format!("Failed to read schema_migrations: {}", e))
                })?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| {
                    MigrateError::Db(format!("Failed to read schema_migrations: {}", e))
                })?;
            let mut map = HashMap::new();
            for row in rows {
                let (name, status) =
                    row.map_err(|e| MigrateError::Db(format!("Row read failed: {}", e)))?;
                map.insert(name, status);
            }
            map
        };

        // Recorded rows the registry does not know about (migrations retired
        // by the application) are tolerated and reported.
        let known: Vec<&str> = self.migrations.iter().map(|m| m.name()).collect();
        report.unknown = recorded
            .keys()
            .filter(|name| !known.contains(&name.as_str()))
            .cloned()
            .collect();
        report.unknown.sort();
        if !report.unknown.is_empty() {
            tracing::info!(
                "schema_migrations has rows for migrations the registry no longer ships \
                 (tolerated, left in place): {:?}",
                report.unknown
            );
        }

        // A recorded failure blocks the run before ANYTHING executes —
        // silently continuing could compound damage on a half-migrated
        // database.
        for migration in &self.migrations {
            if recorded.get(migration.name()).map(String::as_str) == Some("failed") {
                return Err(MigrateError::Blocked(format!(
                    "migration '{}' previously failed and is blocking the run. Fix the \
                     underlying issue, then delete its tracking row so the next run \
                     re-runs it: DELETE FROM schema_migrations WHERE name = '{}'; \
                     (database: {})",
                    migration.name(),
                    migration.name(),
                    self.db_path.display()
                )));
            }
        }

        for migration in &self.migrations {
            if recorded.contains_key(migration.name()) {
                // Only `success` rows reach this point (failures returned
                // above).
                report.skipped.push(migration.name().to_string());
                continue;
            }

            if migration.baseline() && tracking_present && schema_present {
                // Existing database: the schema the baseline would create is
                // already there in some (possibly newer) form. Record it as
                // applied without executing. Both conditions matter: a
                // tracking table WITHOUT a schema means a previous run died
                // after creating the tracking table but before the baseline
                // executed — seeding then would leave the database
                // schemaless and wedge every later migration, so the
                // baseline must run instead.
                record(&conn, migration.name(), "success", None, None)?;
                report.seeded.push(migration.name().to_string());
                tracing::info!(
                    "baseline migration '{}' recorded without executing (existing database)",
                    migration.name()
                );
                continue;
            }

            self.execute_one(&mut conn, migration.as_ref())?;
            report.ran.push(migration.name().to_string());
        }

        Ok(report)
    }

    /// Determine `(tracking_present, schema_present)` without creating the
    /// database file. Without a probe, a tracked database is assumed to
    /// already have its schema.
    fn detect(&self) -> Result<(bool, bool), MigrateError> {
        if !self.db_path.exists() {
            return Ok((false, false));
        }
        let mut conn = Connection::open(&self.db_path).map_err(|e| {
            MigrateError::Db(format!(
                "Failed to open SQLite at {:?}: {}",
                self.db_path, e
            ))
        })?;
        let tracking: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' \
                 AND name = 'schema_migrations'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| MigrateError::Db(format!("Failed to inspect sqlite_master: {}", e)))?;
        let tracking_present = tracking > 0;

        let schema_present = match &self.schema_probe {
            None => tracking_present,
            Some(probe) => {
                // The probe only reads; dropping the transaction rolls back.
                let tx = conn.transaction().map_err(|e| {
                    MigrateError::Db(format!("Failed to start probe transaction: {}", e))
                })?;
                probe(&MigrationTx::new(&tx))?
            }
        };
        Ok((tracking_present, schema_present))
    }

    /// Execute one migration inside its own transaction, applying the
    /// rebuild convention when flagged, and record the outcome row.
    fn execute_one(
        &self,
        conn: &mut Connection,
        migration: &dyn Migration,
    ) -> Result<(), MigrateError> {
        let started = Instant::now();

        // PRAGMA foreign_keys is a no-op inside a transaction, so rebuilds
        // toggle it outside the transaction boundaries.
        if migration.rebuild() {
            conn.execute_batch("PRAGMA foreign_keys = OFF;")
                .map_err(|e| MigrateError::Db(format!("Failed to disable foreign_keys: {}", e)))?;
        }

        let result = run_in_transaction(conn, migration);

        if migration.rebuild() {
            // Best-effort restore regardless of outcome; connections the
            // application opens set their own pragmas.
            let _ = conn.execute_batch("PRAGMA foreign_keys = ON;");
        }

        let duration_ms = started.elapsed().as_millis() as i64;
        match result {
            Ok(()) => {
                record(conn, migration.name(), "success", Some(duration_ms), None)?;
                tracing::info!(
                    "migration '{}' applied in {}ms",
                    migration.name(),
                    duration_ms
                );
                Ok(())
            }
            Err(e) => {
                let error_text = e.to_string();
                record(
                    conn,
                    migration.name(),
                    "failed",
                    Some(duration_ms),
                    Some(&error_text),
                )?;
                Err(MigrateError::MigrationFailed(format!(
                    "migration '{}' failed: {}. Run aborted; the migration was rolled back \
                     and later migrations were not run. After fixing the underlying issue, \
                     delete the tracking row so the next run re-runs it: \
                     DELETE FROM schema_migrations WHERE name = '{}'; (database: {})",
                    migration.name(),
                    error_text,
                    migration.name(),
                    self.db_path.display()
                )))
            }
        }
    }
}

/// Fallback guard message when a probe is configured without one (not
/// reachable through the public builder, which requires both together).
fn default_untracked_schema_error(db_path: &Path) -> String {
    format!(
        "database {} has a schema but no schema_migrations tracking table; \
         refusing to run migrations against it",
        db_path.display()
    )
}

// ── Tracking table ────────────────────────────────────────────────────────────

const TRACKING_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    name        TEXT PRIMARY KEY,
    applied_at  TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,
    error       TEXT
)";

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

/// Pragmas for the runner's connection: foreign keys enforced and WAL
/// journaling, matching what a typical application connection uses so
/// migrations execute under the same rules as production queries.
fn apply_runner_pragmas(conn: &Connection) -> Result<(), MigrateError> {
    // journal_mode is a query-style pragma (it returns the resulting mode
    // as a row), so query_row is required where execute would error.
    conn.query_row("PRAGMA journal_mode = WAL;", [], |_row| Ok(()))
        .map_err(|e| MigrateError::Db(format!("Failed to set journal_mode=WAL: {}", e)))?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
        .map_err(|e| MigrateError::Db(format!("Failed to apply pragmas: {}", e)))?;
    Ok(())
}

/// The transactional body of a migration: open, run the body against the
/// [`MigrationTx`] handle, verify foreign keys for rebuilds, commit. Any
/// error path drops the transaction, which rolls it back.
fn run_in_transaction(
    conn: &mut Connection,
    migration: &dyn Migration,
) -> Result<(), MigrateError> {
    let tx = conn
        .transaction()
        .map_err(|e| MigrateError::Db(format!("Failed to start transaction: {}", e)))?;

    migration.up(&MigrationTx::new(&tx))?;

    if migration.rebuild() {
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

    // ── Scripted test migration ────────────────────────────────────────────────

    type ScriptedBody = Box<dyn Fn(&MigrationTx<'_>) -> Result<(), MigrateError> + Send + Sync>;

    /// A registry entry driven by a closure, so tests can express any body —
    /// from a single SQL batch to SQL-plus-Rust logic.
    struct Scripted {
        name: &'static str,
        baseline: bool,
        rebuild: bool,
        body: ScriptedBody,
    }

    impl Migration for Scripted {
        fn name(&self) -> &'static str {
            self.name
        }
        fn baseline(&self) -> bool {
            self.baseline
        }
        fn rebuild(&self) -> bool {
            self.rebuild
        }
        fn up(&self, tx: &MigrationTx<'_>) -> Result<(), MigrateError> {
            (self.body)(tx)
        }
    }

    /// Migration whose body is a single SQL batch.
    fn sql(name: &'static str, sql: &'static str) -> Box<dyn Migration> {
        code(name, move |tx| tx.execute_batch(sql))
    }

    fn sql_baseline(name: &'static str, sql: &'static str) -> Box<dyn Migration> {
        Box::new(Scripted {
            name,
            baseline: true,
            rebuild: false,
            body: Box::new(move |tx| tx.execute_batch(sql)),
        })
    }

    fn sql_rebuild(name: &'static str, sql: &'static str) -> Box<dyn Migration> {
        Box::new(Scripted {
            name,
            baseline: false,
            rebuild: true,
            body: Box::new(move |tx| tx.execute_batch(sql)),
        })
    }

    /// Migration with an arbitrary Rust body.
    fn code<F>(name: &'static str, f: F) -> Box<dyn Migration>
    where
        F: Fn(&MigrationTx<'_>) -> Result<(), MigrateError> + Send + Sync + 'static,
    {
        Box::new(Scripted {
            name,
            baseline: false,
            rebuild: false,
            body: Box::new(f),
        })
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn db_path(dir: &TempDir) -> PathBuf {
        dir.path().join("app.db")
    }

    fn runner(dir: &TempDir, migrations: Vec<Box<dyn Migration>>) -> Runner {
        Runner::new(db_path(dir)).migrations(migrations)
    }

    /// A runner configured with the standard test probe: the schema is
    /// present when the `app_data` table exists.
    fn probed_runner(dir: &TempDir, migrations: Vec<Box<dyn Migration>>) -> Runner {
        runner(dir, migrations).schema_probe(
            |tx| tx.table_exists("app_data"),
            "untracked schema detected; refusing to run",
        )
    }

    fn open(dir: &TempDir) -> Connection {
        Connection::open(db_path(dir)).expect("open app.db")
    }

    /// Read all tracking rows as (name, status, error), ordered by name.
    fn tracking_rows(dir: &TempDir) -> Vec<(String, String, Option<String>)> {
        let conn = open(dir);
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

    fn delete_row(dir: &TempDir, name: &str) {
        let conn = open(dir);
        conn.execute("DELETE FROM schema_migrations WHERE name = ?", [name])
            .expect("delete row");
    }

    fn insert_row(dir: &TempDir, name: &str, status: &str) {
        let conn = open(dir);
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

    // ── SQL-string API ─────────────────────────────────────────────────────────

    #[test]
    fn migration_tx_api_round_trips_values() {
        let mut conn = Connection::open_in_memory().expect("open");
        let tx = conn.transaction().expect("tx");
        let mtx = MigrationTx::new(&tx);

        mtx.execute_batch("CREATE TABLE t (i INTEGER, f REAL, s TEXT, n TEXT, b BLOB);")
            .expect("batch");
        let affected = mtx
            .execute(
                "INSERT INTO t (i, f, s, n, b) VALUES (?1, ?2, ?3, ?4, ?5)",
                &[
                    SqlValue::from(42i64),
                    SqlValue::from(1.5f64),
                    SqlValue::from("hello"),
                    SqlValue::Null,
                    SqlValue::Blob(vec![1, 2, 3]),
                ],
            )
            .expect("insert");
        assert_eq!(affected, 1);

        let rows = mtx
            .query(
                "SELECT i, f, s, n, b FROM t WHERE s = ?1",
                &[SqlValue::from("hello")],
            )
            .expect("query");
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row[0].as_i64(), Some(42));
        assert_eq!(row[1].as_f64(), Some(1.5));
        assert_eq!(row[2].as_str(), Some("hello"));
        assert!(row[3].is_null());
        assert_eq!(row[4], SqlValue::Blob(vec![1, 2, 3]));
        assert_eq!(row[4].as_blob(), Some(&[1u8, 2, 3][..]));

        assert!(mtx.table_exists("t").expect("probe"));
        assert!(!mtx.table_exists("missing").expect("probe"));
    }

    // ── Fresh run: everything runs once, then everything skips ────────────────

    #[test]
    fn fresh_run_all_migrations_run_once_then_skip() {
        let tmp = TempDir::new().unwrap();
        let make = || {
            vec![
                sql("m001_a", "CREATE TABLE t_a (x INTEGER);"),
                sql("m002_b", "INSERT INTO t_a (x) VALUES (1);"),
            ]
        };

        let report = runner(&tmp, make()).run().unwrap();
        assert_eq!(report.ran, vec!["m001_a", "m002_b"]);
        assert!(report.skipped.is_empty());
        assert!(report.seeded.is_empty());

        let rows = tracking_rows(&tmp);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));

        // Second run: nothing executes (re-running the CREATE would fail).
        let report2 = runner(&tmp, make()).run().unwrap();
        assert_eq!(report2.skipped, vec!["m001_a", "m002_b"]);
        assert!(report2.ran.is_empty());
        assert_eq!(
            count(&open(&tmp), "SELECT COUNT(*) FROM t_a"),
            1,
            "the INSERT must have run exactly once"
        );
    }

    // ── Ordering ───────────────────────────────────────────────────────────────

    #[test]
    fn execution_follows_registry_order() {
        let tmp = TempDir::new().unwrap();
        // Each migration appends its own marker; order is proven by rowid.
        let migrations = vec![
            sql(
                "m001_first",
                "CREATE TABLE trace (step TEXT); INSERT INTO trace VALUES ('first');",
            ),
            sql("m002_second", "INSERT INTO trace VALUES ('second');"),
            sql("m003_third", "INSERT INTO trace VALUES ('third');"),
        ];
        runner(&tmp, migrations).run().unwrap();

        let conn = open(&tmp);
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

    // ── SQL-plus-Rust bodies tracked like any other migration ─────────────────

    #[test]
    fn rust_logic_migration_tracked_skipped_and_rerun_like_any_other() {
        let tmp = TempDir::new().unwrap();
        let invocations = Arc::new(AtomicUsize::new(0));
        let make = |inv: &Arc<AtomicUsize>| {
            let inv = Arc::clone(inv);
            vec![
                sql("m001_sql", "CREATE TABLE t_sql (x INTEGER);"),
                code("m002_logic", move |tx| {
                    // A body mixing queries, Rust-level decisions, and writes.
                    inv.fetch_add(1, Ordering::SeqCst);
                    tx.execute_batch("CREATE TABLE IF NOT EXISTS t_logic (x INTEGER);")?;
                    let rows = tx.query("SELECT COUNT(*) FROM t_logic", &[])?;
                    let existing = rows[0][0].as_i64().unwrap_or(0);
                    if existing == 0 {
                        tx.execute(
                            "INSERT INTO t_logic (x) VALUES (?1)",
                            &[SqlValue::from(7i64)],
                        )?;
                    }
                    Ok(())
                }),
            ]
        };

        // First run: both execute and get identical success rows.
        let report = runner(&tmp, make(&invocations)).run().unwrap();
        assert_eq!(report.ran, vec!["m001_sql", "m002_logic"]);
        let rows = tracking_rows(&tmp);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
        assert_eq!(invocations.load(Ordering::SeqCst), 1);

        // Second run: both skip; the body is not invoked.
        let report2 = runner(&tmp, make(&invocations)).run().unwrap();
        assert_eq!(report2.skipped, vec!["m001_sql", "m002_logic"]);
        assert_eq!(invocations.load(Ordering::SeqCst), 1, "no re-invocation");

        // Deleting its row re-runs it, same rules as every migration.
        delete_row(&tmp, "m002_logic");
        let report3 = runner(&tmp, make(&invocations)).run().unwrap();
        assert_eq!(report3.skipped, vec!["m001_sql"]);
        assert_eq!(report3.ran, vec!["m002_logic"]);
        assert_eq!(invocations.load(Ordering::SeqCst), 2);
    }

    // ── Failure semantics: rollback, failed row, abort, block, recover ────────

    #[test]
    fn sql_failure_rolls_back_records_failed_row_and_aborts() {
        let tmp = TempDir::new().unwrap();
        let make = || {
            vec![
                sql(
                    "m001_ok",
                    "CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);",
                ),
                // The INSERT succeeds, then the script hits a bad statement —
                // the whole migration must roll back, including that INSERT.
                sql(
                    "m002_fail",
                    "INSERT INTO t VALUES (2); INSERT INTO no_such_table VALUES (1);",
                ),
                sql("m003_never", "INSERT INTO t VALUES (3);"),
            ]
        };

        let err = runner(&tmp, make())
            .run()
            .expect_err("must abort on failure");
        let msg = err.to_string();
        assert!(msg.contains("m002_fail"), "error must name the migration");
        assert!(
            msg.contains("no_such_table"),
            "error must include the cause"
        );
        assert!(
            msg.contains("DELETE FROM schema_migrations"),
            "error must describe the recovery procedure"
        );

        // Rollback proof: the partial INSERT from m002_fail is gone.
        let conn = open(&tmp);
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM t"),
            1,
            "m002_fail's partial work must be rolled back; only m001_ok's row remains"
        );
        drop(conn);

        // Failed row recorded with error text; m003_never has no row.
        let rows = tracking_rows(&tmp);
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

        // Second run: the failed row blocks before anything executes.
        let err2 = runner(&tmp, make())
            .run()
            .expect_err("failed row must block");
        assert!(err2.to_string().contains("m002_fail"));
        assert!(err2.to_string().contains("previously failed"));
        assert_eq!(
            count(&open(&tmp), "SELECT COUNT(*) FROM t"),
            1,
            "nothing may execute while a failed row blocks the run"
        );

        // Recovery: delete the failed row (and fix the issue) → re-runs and
        // the rest of the chain completes.
        open(&tmp)
            .execute_batch("CREATE TABLE no_such_table (x INTEGER);")
            .unwrap();
        delete_row(&tmp, "m002_fail");
        let report = runner(&tmp, make())
            .run()
            .expect("must succeed after recovery");
        assert_eq!(report.skipped, vec!["m001_ok"]);
        assert_eq!(report.ran, vec!["m002_fail", "m003_never"]);
        let rows = tracking_rows(&tmp);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
    }

    #[test]
    fn failed_row_blocks_even_when_earlier_migrations_are_pending() {
        let tmp = TempDir::new().unwrap();
        insert_row(&tmp, "m002_late", "failed");

        let migrations = vec![
            sql("m001_early", "CREATE TABLE t_early (x INTEGER);"),
            sql("m002_late", "CREATE TABLE t_late (x INTEGER);"),
        ];
        let err = runner(&tmp, migrations).run().expect_err("must block");
        assert!(err.to_string().contains("m002_late"));

        // The pending earlier migration must NOT have run.
        let conn = open(&tmp);
        let n = count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 't_early'",
        );
        assert_eq!(n, 0, "nothing runs while a failed row blocks the run");
    }

    #[test]
    fn rust_logic_failure_rolls_back_its_transaction() {
        let tmp = TempDir::new().unwrap();
        let migrations = vec![
            sql(
                "m001_setup",
                "CREATE TABLE t (x INTEGER); INSERT INTO t VALUES (1);",
            ),
            code("m002_logic_fail", |tx| {
                // Real writes that must be rolled back when the body errors.
                tx.execute_batch("INSERT INTO t VALUES (2); INSERT INTO t VALUES (3);")?;
                Err(MigrateError::Db("simulated migration failure".to_string()))
            }),
        ];

        let err = runner(&tmp, migrations).run().expect_err("must fail");
        assert!(err.to_string().contains("simulated migration failure"));

        // Rollback proof: the migration's inserts are gone.
        assert_eq!(
            count(&open(&tmp), "SELECT COUNT(*) FROM t"),
            1,
            "migration writes must be rolled back on failure"
        );
        let rows = tracking_rows(&tmp);
        let failed = rows
            .iter()
            .find(|(n, _, _)| n == "m002_logic_fail")
            .unwrap();
        assert_eq!(failed.1, "failed");
        assert!(failed
            .2
            .as_deref()
            .unwrap_or_default()
            .contains("simulated migration failure"));
    }

    #[test]
    fn deleted_success_row_reruns_that_migration() {
        let tmp = TempDir::new().unwrap();
        let make = || {
            vec![
                sql("m001_a", "CREATE TABLE IF NOT EXISTS t (x INTEGER);"),
                sql("m002_b", "INSERT INTO t VALUES (1);"),
            ]
        };
        runner(&tmp, make()).run().unwrap();
        delete_row(&tmp, "m002_b");

        let report = runner(&tmp, make()).run().unwrap();
        assert_eq!(report.skipped, vec!["m001_a"]);
        assert_eq!(report.ran, vec!["m002_b"]);
        assert_eq!(
            count(&open(&tmp), "SELECT COUNT(*) FROM t"),
            2,
            "m002_b ran twice in total"
        );
        assert!(tracking_rows(&tmp)
            .iter()
            .all(|(_, status, _)| status == "success"));
    }

    // ── Unknown recorded rows are tolerated ────────────────────────────────────

    #[test]
    fn unknown_recorded_rows_are_tolerated_and_reported() {
        let tmp = TempDir::new().unwrap();
        insert_row(&tmp, "m001_retired", "success");
        insert_row(&tmp, "m002_retired_too", "success");

        let make = || vec![sql("m003_x", "CREATE TABLE IF NOT EXISTS t (x INTEGER);")];
        let report = runner(&tmp, make()).run().expect("unknown rows tolerated");
        assert_eq!(report.unknown, vec!["m001_retired", "m002_retired_too"]);
        assert_eq!(report.ran, vec!["m003_x"]);

        // Even a FAILED unknown row must not block (the registry no longer
        // ships that migration, so there is nothing to re-run).
        delete_row(&tmp, "m001_retired");
        insert_row(&tmp, "m001_retired", "failed");
        let report2 = runner(&tmp, make())
            .run()
            .expect("unknown failed tolerated");
        assert_eq!(report2.skipped, vec!["m003_x"]);
    }

    // ── Baseline convention + schema probe ─────────────────────────────────────

    #[test]
    fn baseline_executes_on_fresh_database() {
        let tmp = TempDir::new().unwrap();
        let migrations = vec![
            sql_baseline("m000_base", "CREATE TABLE app_data (x INTEGER);"),
            sql("m001_next", "INSERT INTO app_data VALUES (1);"),
        ];
        let report = probed_runner(&tmp, migrations).run().unwrap();
        assert_eq!(report.ran, vec!["m000_base", "m001_next"]);
        assert!(report.seeded.is_empty());
        assert_eq!(count(&open(&tmp), "SELECT COUNT(*) FROM app_data"), 1);
    }

    #[test]
    fn baseline_is_seeded_without_executing_on_tracked_database() {
        let tmp = TempDir::new().unwrap();
        // Simulate an existing tracked database: tracking table with
        // recorded rows, plus its own schema (app_data table present).
        insert_row(&tmp, "m001_next", "success");
        open(&tmp)
            .execute_batch("CREATE TABLE app_data (x INTEGER);")
            .unwrap();

        let migrations = vec![
            // Would fail if executed (duplicate table) — proving it doesn't run.
            sql_baseline("m000_base", "CREATE TABLE app_data (x INTEGER);"),
            sql("m001_next", "INSERT INTO app_data VALUES (1);"),
        ];
        let report = probed_runner(&tmp, migrations).run().unwrap();
        assert_eq!(report.seeded, vec!["m000_base"]);
        assert_eq!(report.skipped, vec!["m001_next"]);
        assert!(report.ran.is_empty());

        let rows = tracking_rows(&tmp);
        let base = rows.iter().find(|(n, _, _)| n == "m000_base").unwrap();
        assert_eq!(base.1, "success", "baseline recorded success either way");
    }

    #[test]
    fn baseline_executes_when_tracking_table_exists_but_schema_is_missing() {
        // Crash window: a previous run created schema_migrations but died
        // before the baseline executed or recorded anything. The next run
        // must EXECUTE the baseline — seeding it here would leave the
        // database schemaless and wedge every later migration.
        let tmp = TempDir::new().unwrap();
        let conn = open(&tmp);
        conn.execute(TRACKING_TABLE_SQL, [])
            .expect("tracking table");
        drop(conn);

        let migrations = vec![
            sql_baseline("m000_base", "CREATE TABLE app_data (x INTEGER);"),
            sql("m001_next", "INSERT INTO app_data VALUES (1);"),
        ];
        let report = probed_runner(&tmp, migrations).run().unwrap();
        assert_eq!(
            report.ran,
            vec!["m000_base", "m001_next"],
            "baseline must execute, not seed, when the schema is absent"
        );
        assert!(report.seeded.is_empty());
        assert_eq!(
            count(&open(&tmp), "SELECT COUNT(*) FROM app_data"),
            1,
            "the full chain must have run against the freshly-created schema"
        );
    }

    #[test]
    fn baseline_without_probe_seeds_whenever_tracking_exists() {
        // Documented default: without a probe, a tracked database is assumed
        // to already have its schema.
        let tmp = TempDir::new().unwrap();
        insert_row(&tmp, "m001_next", "success");

        let migrations = vec![
            sql_baseline("m000_base", "CREATE TABLE app_data (x INTEGER);"),
            sql("m001_next", "INSERT INTO app_data VALUES (1);"),
        ];
        let report = runner(&tmp, migrations).run().unwrap();
        assert_eq!(report.seeded, vec!["m000_base"]);
        assert!(report.ran.is_empty());
    }

    #[test]
    fn untracked_schema_is_rejected_with_the_configured_message() {
        let tmp = TempDir::new().unwrap();
        // A database with a schema but no tracking table.
        open(&tmp)
            .execute_batch("CREATE TABLE app_data (x INTEGER);")
            .unwrap();

        let migrations = vec![sql_baseline(
            "m000_base",
            "CREATE TABLE app_data (x INTEGER);",
        )];
        let err = probed_runner(&tmp, migrations)
            .run()
            .expect_err("must reject");
        assert_eq!(
            err.to_string(),
            "untracked schema detected; refusing to run",
            "the caller-supplied guard message must be returned verbatim"
        );
        // Nothing recorded, nothing executed.
        let conn = open(&tmp);
        let n = count(
            &conn,
            "SELECT COUNT(*) FROM sqlite_master WHERE name = 'schema_migrations'",
        );
        assert_eq!(n, 0, "the tracking table must not be created");
    }

    // ── Rebuild convention ─────────────────────────────────────────────────────

    /// Parent/child pair with a real FK edge and data on both sides.
    fn setup_fk_pair(dir: &TempDir) {
        let conn = open(dir);
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
        setup_fk_pair(&tmp);

        let migrations = vec![sql_rebuild(
            "m001_rebuild",
            "CREATE TABLE parent_new (id TEXT PRIMARY KEY, extra INTEGER NOT NULL DEFAULT 0);
             INSERT INTO parent_new (id) SELECT id FROM parent;
             DROP TABLE parent;
             ALTER TABLE parent_new RENAME TO parent;",
        )];
        let report = runner(&tmp, migrations)
            .run()
            .expect("rebuild must succeed");
        assert_eq!(report.ran, vec!["m001_rebuild"]);

        let conn = open(&tmp);
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
        setup_fk_pair(&tmp);

        // This rebuild drops the parent rows, orphaning the child — the
        // runner's foreign_key_check must catch it and roll everything back.
        let migrations = vec![sql_rebuild(
            "m001_bad_rebuild",
            "CREATE TABLE parent_new (id TEXT PRIMARY KEY);
             DROP TABLE parent;
             ALTER TABLE parent_new RENAME TO parent;",
        )];
        let err = runner(&tmp, migrations)
            .run()
            .expect_err("violation must fail");
        assert!(err.to_string().contains("foreign-key violation"));

        // Rollback proof: the original parent (with its row) is intact.
        let conn = open(&tmp);
        assert_eq!(
            count(&conn, "SELECT COUNT(*) FROM parent"),
            1,
            "the original parent table and row must survive the rollback"
        );
        drop(conn);

        let rows = tracking_rows(&tmp);
        let failed = rows
            .iter()
            .find(|(n, _, _)| n == "m001_bad_rebuild")
            .unwrap();
        assert_eq!(failed.1, "failed");
    }

    #[test]
    fn foreign_keys_enforcement_is_restored_after_a_rebuild() {
        let tmp = TempDir::new().unwrap();
        setup_fk_pair(&tmp);

        // The migration AFTER the rebuild violates the FK — it must fail,
        // proving the runner re-enabled foreign_keys on its connection once
        // the rebuild committed.
        let migrations = vec![
            sql_rebuild(
                "m001_rebuild",
                "CREATE TABLE parent_new (id TEXT PRIMARY KEY);
                 INSERT INTO parent_new (id) SELECT id FROM parent;
                 DROP TABLE parent;
                 ALTER TABLE parent_new RENAME TO parent;",
            ),
            sql(
                "m002_violate",
                "INSERT INTO child VALUES ('c2', 'no_such_parent');",
            ),
        ];
        let err = runner(&tmp, migrations)
            .run()
            .expect_err("FK violation after the rebuild must fail");
        assert!(err.to_string().contains("m002_violate"));

        let rows = tracking_rows(&tmp);
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
}
