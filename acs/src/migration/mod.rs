//! Numbered migration system with database-backed run tracking.
//!
//! # Design
//!
//! Each migration is a numbered file: `mNNN_<name>.rs` (the `m` prefix is
//! required because Rust module names cannot start with a digit). The struct
//! inside implements the [`Migration`] trait, and is registered **in order**
//! in the [`registry`] function.
//!
//! Execution is tracked by name in a `schema_migrations` table inside
//! `<data_dir>/acs.db`. The table is created by the runner itself — not by a
//! migration — before any migration logic runs:
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
//! [`run_pending`] walks the registry in order. The tracking table — and only
//! the tracking table — decides what executes:
//!
//! - **no row** → run the migration, then record `success` or `failed`;
//! - **`success` row** → skip without invoking the migration;
//! - **`failed` row** → abort startup with an error naming the migration.
//!   The sanctioned recovery workflow is: fix the underlying issue, then
//!   delete the row (`DELETE FROM schema_migrations WHERE name = '<name>';`)
//!   so the next startup re-runs it.
//!
//! A migration that runs but finds nothing to do still records `success` —
//! the row means "processed", so it never re-runs. Each migration keeps its
//! own internal idempotency checks purely as a safety property: they make
//! delete-row-and-re-run harmless, but the runner never consults them to
//! decide execution.
//!
//! When a migration fails, its row is recorded with status `failed` and the
//! error text, and the runner aborts immediately — later migrations do not
//! run and get no row.
//!
//! # Backfill for databases that predate the tracking table
//!
//! Older installs recorded applied migrations in `<data_dir>/migrations.json`.
//! The first time the runner creates the tracking table it seeds a `success`
//! row for every registry migration named in that file — those are treated as
//! already applied without being invoked. Every other migration then runs
//! normally; on an already-migrated database their internal idempotency makes
//! that a safe no-op, which is recorded as `success`. After this one-time
//! backfill the table is the sole source of truth; `migrations.json` is left
//! on disk but never written again.
//!
//! # Adding a new migration
//!
//! 1. Create `src/migration/mNNN_<name>.rs` (increment NNN).
//! 2. Implement [`Migration`] for a unit struct in that file.
//! 3. Append one `Box::new(mNNN_<name>::YourStruct)` line to [`registry`].

pub mod legacy_types;
mod m001_jobs_to_workflows;
mod m002_json_to_sqlite;
mod m003_drop_step_output_summary;
mod m004_drop_input_schema;
mod m005_shell_claude_to_agent;
mod m006_agent_step_normalize;
mod m007_add_token_columns;
mod m008_add_workflow_deleted;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

use async_trait::async_trait;
use rusqlite::{Connection, OptionalExtension};
use serde::Deserialize;

use crate::errors::AcsError;

// ── Migration trait ───────────────────────────────────────────────────────────

/// A single forward migration step.
///
/// Each migration lives in a numbered file (`mNNN_<name>.rs`). The name must
/// be stable — it is the primary key in the `schema_migrations` tracking
/// table and must never be changed after it ships.
#[async_trait]
pub trait Migration: Send + Sync {
    /// The stable migration name, e.g. `"m001_jobs_to_workflows"`.
    fn name(&self) -> &'static str;

    /// Run the migration against the given data directory.
    ///
    /// Returns `Ok(true)` if the migration performed work, `Ok(false)` if it
    /// determined there was nothing to do. The runner records `success`
    /// either way — the return value is informational (logging) only.
    ///
    /// **Idempotency**: re-running on already-migrated data must be a no-op.
    /// The runner does not rely on this to decide execution (the tracking
    /// table does), but it is what makes the delete-row-and-re-run recovery
    /// workflow safe.
    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError>;
}

// ── Registry ──────────────────────────────────────────────────────────────────

/// All available migrations **in order**. Append new migrations here.
fn registry() -> Vec<Box<dyn Migration>> {
    vec![
        Box::new(m001_jobs_to_workflows::JobsToWorkflows),
        Box::new(m002_json_to_sqlite::JsonToSqlite),
        Box::new(m003_drop_step_output_summary::DropStepOutputSummary),
        Box::new(m004_drop_input_schema::DropInputSchema),
        Box::new(m005_shell_claude_to_agent::ShellClaudeToAgent),
        Box::new(m006_agent_step_normalize::AgentStepNormalize),
        Box::new(m007_add_token_columns::AddTokenColumns),
        Box::new(m008_add_workflow_deleted::AddWorkflowDeleted),
    ]
}

// ── Run report ────────────────────────────────────────────────────────────────

/// Summary returned by [`run_pending`].
#[derive(Debug, Default)]
pub struct MigrationRunReport {
    /// Backfilled as `success` from the legacy `migrations.json` state when
    /// the tracking table was first created. Not invoked.
    pub seeded: Vec<String>,
    /// Skipped because the tracking table already records them as `success`.
    /// Not invoked.
    pub skipped: Vec<String>,
    /// Invoked and performed work. Recorded as `success`.
    pub ran: Vec<String>,
    /// Invoked but found nothing to do. Still recorded as `success`.
    pub ran_no_op: Vec<String>,
}

// ── Tracking table access ─────────────────────────────────────────────────────

const TRACKING_TABLE_SQL: &str = "CREATE TABLE IF NOT EXISTS schema_migrations (
    name        TEXT PRIMARY KEY,
    applied_at  TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('success','failed')),
    duration_ms INTEGER,
    error       TEXT
)";

/// Open the tracking database, creating the file and the `schema_migrations`
/// table if they do not exist yet.
fn open_tracking_db(db_path: &Path) -> Result<Connection, AcsError> {
    let conn = Connection::open(db_path)
        .map_err(|e| AcsError::Storage(format!("Failed to open SQLite at {:?}: {}", db_path, e)))?;
    conn.execute(TRACKING_TABLE_SQL, []).map_err(|e| {
        AcsError::Storage(format!("Failed to create schema_migrations table: {}", e))
    })?;
    Ok(conn)
}

/// Run a closure against the tracking database on a blocking task. Opens a
/// fresh connection (creating the table if needed) — these are low-frequency
/// startup operations, so per-call connections keep the runner simple.
async fn with_tracking_db<F, T>(db_path: &Path, f: F) -> Result<T, AcsError>
where
    F: FnOnce(&Connection) -> Result<T, AcsError> + Send + 'static,
    T: Send + 'static,
{
    let path: PathBuf = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let conn = open_tracking_db(&path)?;
        f(&conn)
    })
    .await
    .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?
}

/// Return `true` when the tracking table already exists, **without** creating
/// the database file or the table. Used to detect a first run so the legacy
/// backfill happens exactly once.
async fn tracking_table_present(db_path: &Path) -> Result<bool, AcsError> {
    if !db_path.exists() {
        return Ok(false);
    }
    let path: PathBuf = db_path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let conn = Connection::open(&path).map_err(|e| {
            AcsError::Storage(format!("Failed to open SQLite at {:?}: {}", path, e))
        })?;
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master \
                 WHERE type = 'table' AND name = 'schema_migrations'",
                [],
                |r| r.get(0),
            )
            .map_err(|e| AcsError::Storage(format!("Failed to inspect sqlite_master: {}", e)))?;
        Ok(n > 0)
    })
    .await
    .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?
}

/// Fetch the recorded status (`"success"` / `"failed"`) for a migration, or
/// `None` when it has no row.
async fn migration_status(db_path: &Path, name: &str) -> Result<Option<String>, AcsError> {
    let name = name.to_string();
    with_tracking_db(db_path, move |conn| {
        conn.query_row(
            "SELECT status FROM schema_migrations WHERE name = ?",
            [&name],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .map_err(|e| AcsError::Storage(format!("Failed to read schema_migrations: {}", e)))
    })
    .await
}

/// Record (or overwrite) a migration's tracking row.
async fn record_migration(
    db_path: &Path,
    name: &str,
    status: &str,
    duration_ms: Option<i64>,
    error: Option<String>,
) -> Result<(), AcsError> {
    let name = name.to_string();
    let status = status.to_string();
    let applied_at = chrono::Utc::now().to_rfc3339();
    with_tracking_db(db_path, move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO schema_migrations \
             (name, applied_at, status, duration_ms, error) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![name, applied_at, status, duration_ms, error],
        )
        .map_err(|e| AcsError::Storage(format!("Failed to write schema_migrations: {}", e)))?;
        Ok(())
    })
    .await
}

/// Insert `success` rows for the given names, skipping any that already have
/// a row. Used only by the one-time legacy backfill.
async fn seed_success_rows(db_path: &Path, names: Vec<String>) -> Result<(), AcsError> {
    let applied_at = chrono::Utc::now().to_rfc3339();
    with_tracking_db(db_path, move |conn| {
        for name in &names {
            conn.execute(
                "INSERT OR IGNORE INTO schema_migrations \
                 (name, applied_at, status, duration_ms, error) \
                 VALUES (?1, ?2, 'success', NULL, NULL)",
                rusqlite::params![name, applied_at],
            )
            .map_err(|e| AcsError::Storage(format!("Failed to seed schema_migrations: {}", e)))?;
        }
        Ok(())
    })
    .await
}

// ── Public runner ─────────────────────────────────────────────────────────────

/// Run all pending migrations in order, tracked by the `schema_migrations`
/// table (see the module documentation for the exact semantics).
pub async fn run_pending(data_dir: &Path) -> Result<MigrationRunReport, AcsError> {
    run_with_registry(data_dir, registry()).await
}

/// Runner core, parameterised over the registry so tests can drive it with
/// mock migrations.
async fn run_with_registry(
    data_dir: &Path,
    migrations: Vec<Box<dyn Migration>>,
) -> Result<MigrationRunReport, AcsError> {
    let db_path = data_dir.join("acs.db");
    let mut report = MigrationRunReport::default();

    // Create the tracking table before any migration logic runs. When the
    // table did not exist yet, this is the first run of the tracked runner
    // against this data dir — backfill from the legacy state file so
    // already-applied migrations are not invoked again.
    let had_tracking_table = tracking_table_present(&db_path).await?;
    with_tracking_db(&db_path, |_| Ok(())).await?;

    if !had_tracking_table {
        let legacy_applied = read_legacy_state(data_dir).await?;
        if !legacy_applied.is_empty() {
            let seed: Vec<String> = migrations
                .iter()
                .map(|m| m.name().to_string())
                .filter(|n| legacy_applied.contains(n))
                .collect();
            if !seed.is_empty() {
                seed_success_rows(&db_path, seed.clone()).await?;
                report.seeded = seed;
            }
        }
    }

    for migration in &migrations {
        let name = migration.name().to_string();
        match migration_status(&db_path, &name).await?.as_deref() {
            Some("success") => {
                report.skipped.push(name);
            }
            Some(_) => {
                // A recorded failure blocks startup until an operator
                // intervenes; silently re-running could compound damage on a
                // half-migrated database.
                return Err(AcsError::Internal(format!(
                    "migration '{}' previously failed and is blocking startup. Fix the \
                     underlying issue, then delete its tracking row so the next startup \
                     re-runs it: DELETE FROM schema_migrations WHERE name = '{}'; \
                     (database: {})",
                    name,
                    name,
                    db_path.display()
                )));
            }
            None => {
                let started = Instant::now();
                let outcome = migration.run(data_dir).await;
                let duration_ms = started.elapsed().as_millis() as i64;
                match outcome {
                    Ok(did_work) => {
                        record_migration(&db_path, &name, "success", Some(duration_ms), None)
                            .await?;
                        if did_work {
                            report.ran.push(name);
                        } else {
                            report.ran_no_op.push(name);
                        }
                    }
                    Err(e) => {
                        let error_text = e.to_string();
                        record_migration(
                            &db_path,
                            &name,
                            "failed",
                            Some(duration_ms),
                            Some(error_text.clone()),
                        )
                        .await?;
                        return Err(AcsError::Internal(format!(
                            "migration '{}' failed: {}. Startup aborted; later migrations \
                             were not run. After fixing the underlying issue, delete the \
                             tracking row so the next startup re-runs it: \
                             DELETE FROM schema_migrations WHERE name = '{}';",
                            name, error_text, name
                        )));
                    }
                }
            }
        }
    }

    Ok(report)
}

// ── Legacy state file (read-only, for the one-time backfill) ──────────────────

/// On-disk shape of the legacy `migrations.json` tracking file.
#[derive(Debug, Deserialize, Default)]
struct LegacyMigrationState {
    applied: Vec<String>,
}

/// Read the applied-migration names from the legacy `<data_dir>/migrations.json`.
/// Used only to backfill the `schema_migrations` table the first time the
/// tracked runner sees a data dir; the file is never written again.
///
/// - If the file does not exist, returns an empty set (fresh install).
/// - If the file is corrupt (invalid JSON), logs a warning and returns an
///   empty set — every migration then runs normally, and their internal
///   idempotency keeps that safe on an already-migrated database.
async fn read_legacy_state(data_dir: &Path) -> Result<HashSet<String>, AcsError> {
    let path = data_dir.join("migrations.json");

    if !path.exists() {
        return Ok(HashSet::new());
    }

    match tokio::fs::read_to_string(&path).await {
        Err(e) => {
            tracing::warn!(
                "Could not read migrations.json ({}): {}. Treating as empty.",
                path.display(),
                e
            );
            Ok(HashSet::new())
        }
        Ok(content) => match serde_json::from_str::<LegacyMigrationState>(&content) {
            Ok(state) => Ok(state.applied.into_iter().collect()),
            Err(e) => {
                tracing::warn!(
                    "migrations.json is corrupt ({}): {}. Treating as empty.",
                    path.display(),
                    e
                );
                Ok(HashSet::new())
            }
        },
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    // ── Mock migrations ───────────────────────────────────────────────────────

    /// What a [`Scripted`] mock does when invoked.
    #[derive(Clone)]
    enum Behavior {
        /// Return `Ok(true)` — "ran and did work".
        DidWork,
        /// Return `Ok(false)` — "ran, nothing to do".
        NoOp,
        /// Return an error while the flag is `true`, `Ok(true)` afterwards.
        FailWhile(Arc<AtomicBool>),
    }

    /// A mock migration that records every invocation into a shared log.
    struct Scripted {
        name: &'static str,
        behavior: Behavior,
        invocations: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Migration for Scripted {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn run(&self, _data_dir: &Path) -> Result<bool, AcsError> {
            self.invocations.lock().unwrap().push(self.name.to_string());
            match &self.behavior {
                Behavior::DidWork => Ok(true),
                Behavior::NoOp => Ok(false),
                Behavior::FailWhile(flag) => {
                    if flag.load(Ordering::SeqCst) {
                        Err(AcsError::Storage("simulated migration failure".to_string()))
                    } else {
                        Ok(true)
                    }
                }
            }
        }
    }

    fn scripted(
        name: &'static str,
        behavior: Behavior,
        log: &Arc<Mutex<Vec<String>>>,
    ) -> Box<dyn Migration> {
        Box::new(Scripted {
            name,
            behavior,
            invocations: Arc::clone(log),
        })
    }

    // ── Tracking-table helpers for assertions ─────────────────────────────────

    /// Read all rows as (name, status, error), ordered by name.
    fn tracking_rows(data_dir: &Path) -> Vec<(String, String, Option<String>)> {
        let conn = Connection::open(data_dir.join("acs.db")).expect("open acs.db");
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
        let conn = Connection::open(data_dir.join("acs.db")).expect("open acs.db");
        conn.execute("DELETE FROM schema_migrations WHERE name = ?", [name])
            .expect("delete row");
    }

    fn write_legacy_state(data_dir: &Path, applied: &[&str]) {
        let json = serde_json::json!({ "applied": applied }).to_string();
        std::fs::write(data_dir.join("migrations.json"), json).expect("write migrations.json");
    }

    // ── Legacy state file reading ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_legacy_state_read_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let state = read_legacy_state(tmp.path()).await.unwrap();
        assert!(state.is_empty());
    }

    #[tokio::test]
    async fn test_legacy_state_corruption_treated_as_empty() {
        let tmp = TempDir::new().unwrap();
        tokio::fs::write(tmp.path().join("migrations.json"), b"not valid json {{{{")
            .await
            .unwrap();
        let state = read_legacy_state(tmp.path()).await.unwrap();
        assert!(
            state.is_empty(),
            "corrupt state file should be treated as empty"
        );
    }

    // ── Fresh run: everything runs once, then everything skips ────────────────

    #[tokio::test]
    async fn test_fresh_run_all_migrations_run_once_then_skip() {
        let tmp = TempDir::new().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let make = |log: &Arc<Mutex<Vec<String>>>| {
            vec![
                scripted("m001_a", Behavior::DidWork, log),
                scripted("m002_b", Behavior::NoOp, log),
                scripted("m003_c", Behavior::DidWork, log),
            ]
        };

        let report = run_with_registry(tmp.path(), make(&log)).await.unwrap();
        assert_eq!(
            *log.lock().unwrap(),
            vec!["m001_a", "m002_b", "m003_c"],
            "all migrations must be invoked, in order"
        );
        assert_eq!(report.ran, vec!["m001_a", "m003_c"]);
        assert_eq!(
            report.ran_no_op,
            vec!["m002_b"],
            "a no-op run is still processed"
        );
        assert!(report.skipped.is_empty());
        assert!(report.seeded.is_empty());

        // Every migration recorded success — including the no-op.
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));

        // Second run: nothing is invoked; everything skips via the table.
        let report2 = run_with_registry(tmp.path(), make(&log)).await.unwrap();
        assert_eq!(
            log.lock().unwrap().len(),
            3,
            "second run must not invoke any migration"
        );
        assert_eq!(report2.skipped, vec!["m001_a", "m002_b", "m003_c"]);
        assert!(report2.ran.is_empty());
        assert!(report2.ran_no_op.is_empty());
    }

    // ── Backfill from legacy migrations.json ──────────────────────────────────

    #[tokio::test]
    async fn test_backfill_seeds_legacy_applied_and_runs_the_rest() {
        let tmp = TempDir::new().unwrap();
        write_legacy_state(tmp.path(), &["m001_a", "m002_b"]);

        let log = Arc::new(Mutex::new(Vec::new()));
        let registry = vec![
            scripted("m001_a", Behavior::DidWork, &log),
            scripted("m002_b", Behavior::DidWork, &log),
            scripted("m003_c", Behavior::DidWork, &log),
        ];

        let report = run_with_registry(tmp.path(), registry).await.unwrap();
        assert_eq!(
            report.seeded,
            vec!["m001_a", "m002_b"],
            "legacy-applied migrations must be seeded, not run"
        );
        assert_eq!(report.ran, vec!["m003_c"]);
        assert_eq!(
            *log.lock().unwrap(),
            vec!["m003_c"],
            "seeded migrations must not be invoked"
        );

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
    }

    #[tokio::test]
    async fn test_backfill_happens_only_once() {
        let tmp = TempDir::new().unwrap();
        // First run with no legacy file creates the tracking table.
        let log = Arc::new(Mutex::new(Vec::new()));
        run_with_registry(
            tmp.path(),
            vec![scripted("m001_a", Behavior::DidWork, &log)],
        )
        .await
        .unwrap();

        // A legacy file appearing *after* the table exists must be ignored —
        // the table is the sole source of truth from then on.
        write_legacy_state(tmp.path(), &["m002_b"]);
        let report = run_with_registry(
            tmp.path(),
            vec![
                scripted("m001_a", Behavior::DidWork, &log),
                scripted("m002_b", Behavior::DidWork, &log),
            ],
        )
        .await
        .unwrap();
        assert!(
            report.seeded.is_empty(),
            "no re-seeding once the tracking table exists"
        );
        assert_eq!(report.ran, vec!["m002_b"], "m002_b must actually run");
    }

    #[tokio::test]
    async fn test_backfill_ignores_legacy_names_not_in_registry() {
        let tmp = TempDir::new().unwrap();
        write_legacy_state(tmp.path(), &["m001_a", "m999_retired"]);

        let log = Arc::new(Mutex::new(Vec::new()));
        let report = run_with_registry(
            tmp.path(),
            vec![scripted("m001_a", Behavior::DidWork, &log)],
        )
        .await
        .unwrap();
        assert_eq!(report.seeded, vec!["m001_a"]);
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 1, "unknown legacy names are not seeded");
    }

    // ── Deleted row re-runs ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_deleted_success_row_reruns_that_migration() {
        let tmp = TempDir::new().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let make = |log: &Arc<Mutex<Vec<String>>>| {
            vec![
                scripted("m001_a", Behavior::DidWork, log),
                scripted("m002_b", Behavior::NoOp, log),
            ]
        };

        run_with_registry(tmp.path(), make(&log)).await.unwrap();
        delete_row(tmp.path(), "m002_b");

        let report = run_with_registry(tmp.path(), make(&log)).await.unwrap();
        assert_eq!(report.skipped, vec!["m001_a"]);
        assert_eq!(
            report.ran_no_op,
            vec!["m002_b"],
            "the migration with the deleted row must re-run"
        );
        assert_eq!(
            *log.lock().unwrap(),
            vec!["m001_a", "m002_b", "m002_b"],
            "m002_b invoked twice in total, m001_a once"
        );
        // Row recorded success again.
        let rows = tracking_rows(tmp.path());
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
    }

    // ── Failure handling ───────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_failure_records_error_aborts_and_blocks_later_migrations() {
        let tmp = TempDir::new().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let failing = Arc::new(AtomicBool::new(true));
        let make = |log: &Arc<Mutex<Vec<String>>>, flag: &Arc<AtomicBool>| {
            vec![
                scripted("m001_ok", Behavior::DidWork, log),
                scripted("m002_fail", Behavior::FailWhile(Arc::clone(flag)), log),
                scripted("m003_never", Behavior::DidWork, log),
            ]
        };

        // First startup: m001 succeeds, m002 fails and aborts, m003 never runs.
        let err = run_with_registry(tmp.path(), make(&log, &failing))
            .await
            .expect_err("must abort on failure");
        let msg = err.to_string();
        assert!(msg.contains("m002_fail"), "error must name the migration");
        assert!(
            msg.contains("simulated migration failure"),
            "error must include the cause"
        );
        assert!(
            msg.contains("DELETE FROM schema_migrations"),
            "error must describe the recovery workflow"
        );
        assert_eq!(*log.lock().unwrap(), vec!["m001_ok", "m002_fail"]);

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 2, "m003_never must have no row");
        let failed = rows.iter().find(|(n, _, _)| n == "m002_fail").unwrap();
        assert_eq!(failed.1, "failed");
        assert!(
            failed
                .2
                .as_deref()
                .unwrap_or_default()
                .contains("simulated migration failure"),
            "failed row must carry the error text"
        );

        // Second startup: the failed row blocks startup without invoking
        // anything at all.
        let err2 = run_with_registry(tmp.path(), make(&log, &failing))
            .await
            .expect_err("failed row must block startup");
        assert!(err2.to_string().contains("m002_fail"));
        assert!(err2.to_string().contains("previously failed"));
        assert_eq!(
            log.lock().unwrap().len(),
            2,
            "no migration may be invoked while a failed row blocks startup"
        );

        // Recovery: fix the issue (flip the flag), delete the failed row —
        // the migration re-runs and the rest of the chain completes.
        failing.store(false, Ordering::SeqCst);
        delete_row(tmp.path(), "m002_fail");
        let report = run_with_registry(tmp.path(), make(&log, &failing))
            .await
            .expect("must succeed after recovery");
        assert_eq!(report.skipped, vec!["m001_ok"]);
        assert_eq!(report.ran, vec!["m002_fail", "m003_never"]);
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
    }

    // ── Ordering ───────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_registry_order_is_execution_order() {
        let tmp = TempDir::new().unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        // Names deliberately not in lexicographic order — the runner must
        // follow registry order, not name order.
        let registry = vec![
            scripted("m_zulu", Behavior::DidWork, &log),
            scripted("m_alpha", Behavior::DidWork, &log),
            scripted("m_mike", Behavior::DidWork, &log),
        ];
        run_with_registry(tmp.path(), registry).await.unwrap();
        assert_eq!(*log.lock().unwrap(), vec!["m_zulu", "m_alpha", "m_mike"]);
    }

    // ── Real registry ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_run_pending_real_registry_fresh_install() {
        // Fresh install: every registered migration is invoked once and
        // recorded success; acs.db ends up with the full schema.
        let tmp = TempDir::new().unwrap();
        let report = run_pending(tmp.path()).await.unwrap();

        assert!(report.seeded.is_empty());
        assert!(report.skipped.is_empty());
        let processed = report.ran.len() + report.ran_no_op.len();
        assert_eq!(
            processed, 8,
            "all 8 registered migrations must be processed"
        );
        assert!(
            report.ran.contains(&"m002_json_to_sqlite".to_string()),
            "m002 creates the database schema on a fresh install"
        );
        assert!(
            tmp.path().join("acs.db").exists(),
            "acs.db must exist after a fresh-install run"
        );

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 8);
        assert!(
            rows.iter().all(|(_, status, _)| status == "success"),
            "a fresh install must end with all migrations recorded as success"
        );

        // Second startup: everything skips; nothing new is recorded.
        let report2 = run_pending(tmp.path()).await.unwrap();
        assert_eq!(report2.skipped.len(), 8);
        assert!(report2.ran.is_empty());
        assert!(report2.ran_no_op.is_empty());
    }

    /// Build a data dir that simulates an install from before the tracking
    /// table existed: an old-shape acs.db (inline UNIQUE on workflows.name,
    /// no `deleted` column) plus a migrations.json listing everything up to
    /// and including m007 as applied.
    fn build_legacy_data_dir(data_dir: &Path) {
        let conn = Connection::open(data_dir.join("acs.db")).expect("create legacy acs.db");
        conn.execute_batch(
            "CREATE TABLE workflows (
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
             CREATE TABLE workflow_runs (
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
             CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             INSERT INTO workflows (id, name, version, schedule, schedule_mode,
                 enabled, steps_json, allow_concurrent, on_failure, created_at, updated_at)
             VALUES ('id-1', 'legacy-wf', 1, '* * * * *', 'cron', 1, '[]', 1, 'abort',
                 '2025-01-01T00:00:00Z', '2025-01-01T00:00:00Z');",
        )
        .expect("apply legacy schema");
        write_legacy_state(
            data_dir,
            &[
                "m001_jobs_to_workflows",
                "m002_json_to_sqlite",
                "m003_drop_step_output_summary",
                "m004_drop_input_schema",
                "m005_shell_claude_to_agent",
                "m006_agent_step_normalize",
                "m007_add_token_columns",
            ],
        );
    }

    #[tokio::test]
    async fn test_run_pending_real_registry_legacy_backfill() {
        let tmp = TempDir::new().unwrap();
        build_legacy_data_dir(tmp.path());

        let report = run_pending(tmp.path()).await.unwrap();
        assert_eq!(
            report.seeded.len(),
            7,
            "m001..m007 must be seeded from migrations.json"
        );
        assert_eq!(
            report.ran,
            vec!["m008_add_workflow_deleted"],
            "only the migration missing from the legacy state runs"
        );

        // m008 really executed: the inline UNIQUE is gone and the data survived.
        let conn = Connection::open(tmp.path().join("acs.db")).expect("open");
        let ddl: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'workflows'",
                [],
                |r| r.get(0),
            )
            .expect("read DDL");
        assert!(
            !ddl.to_uppercase().contains("UNIQUE"),
            "m008 must have rebuilt the workflows table"
        );
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM workflows", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 1, "legacy workflow row must survive");
        drop(conn);

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));

        // Deleting a success row re-runs that migration on the next startup;
        // its internal idempotency makes the re-run a safe no-op.
        delete_row(tmp.path(), "m008_add_workflow_deleted");
        let report2 = run_pending(tmp.path()).await.unwrap();
        assert_eq!(report2.skipped.len(), 7);
        assert_eq!(report2.ran_no_op, vec!["m008_add_workflow_deleted"]);
        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));
    }

    /// A legacy database whose migrations.json was lost entirely: every
    /// migration runs, each no-ops via its internal idempotency, and all are
    /// recorded success.
    #[tokio::test]
    async fn test_run_pending_real_registry_legacy_without_state_file() {
        let tmp = TempDir::new().unwrap();
        build_legacy_data_dir(tmp.path());
        std::fs::remove_file(tmp.path().join("migrations.json")).expect("remove state file");

        let report = run_pending(tmp.path()).await.unwrap();
        assert!(report.seeded.is_empty(), "nothing to seed without the file");
        assert!(report.skipped.is_empty());
        assert_eq!(report.ran.len() + report.ran_no_op.len(), 8);

        let rows = tracking_rows(tmp.path());
        assert_eq!(rows.len(), 8);
        assert!(rows.iter().all(|(_, status, _)| status == "success"));

        // The data survived the full pass.
        let conn = Connection::open(tmp.path().join("acs.db")).expect("open");
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM workflows", [], |r| r.get(0))
            .expect("count");
        assert_eq!(n, 1, "legacy workflow row must survive a full re-run");
    }
}
