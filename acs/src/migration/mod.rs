//! Numbered migration system for the Agent Cron Scheduler.
//!
//! # Design
//!
//! Each migration is a numbered file: `mNNN_<name>.rs` (the `m` prefix is
//! required because Rust module names cannot start with a digit). The struct
//! inside implements the [`Migration`] trait, and is registered in the
//! [`registry`] function in order.
//!
//! Applied migration names are tracked in `<data_dir>/migrations.json`:
//!
//! ```json
//! { "applied": ["m001_jobs_to_workflows"] }
//! ```
//!
//! At daemon startup, call [`run_pending`] to execute any migrations that have
//! not yet been applied. Each migration's [`Migration::run`] returns
//! `Ok(true)` when it did work, or `Ok(false)` when it determined there was
//! nothing to do (idempotent). The runner only adds the migration to the
//! applied set when `run` returns `Ok(true)`.
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
mod m008_cost_ledger_cascade;

use std::collections::HashSet;
use std::path::Path;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::errors::AcsError;

// ── Migration trait ───────────────────────────────────────────────────────────

/// A single forward migration step.
///
/// Each migration lives in a numbered file (`mNNN_<name>.rs`). The name must
/// be stable — it is used as the identifier in the applied-migrations state
/// file and must never be changed after it ships.
#[async_trait]
pub trait Migration: Send + Sync {
    /// The stable migration name, e.g. `"m001_jobs_to_workflows"`.
    fn name(&self) -> &'static str;

    /// Run the migration against the given data directory.
    ///
    /// **Idempotent contract**: re-running on already-migrated data must be a
    /// no-op. The implementation is responsible for detecting that it has
    /// already been applied (e.g., checking for the output file).
    ///
    /// Returns `Ok(true)` if the migration performed work, `Ok(false)` if it
    /// determined there was nothing to do (already applied or not needed).
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
        Box::new(m008_cost_ledger_cascade::CostLedgerCascade),
    ]
}

// ── Run report ────────────────────────────────────────────────────────────────

/// Summary returned by [`run_pending`].
#[derive(Debug, Default)]
pub struct MigrationRunReport {
    /// Migrations that were already in the applied set — skipped entirely.
    pub already_applied: Vec<String>,
    /// Migrations that ran and returned `Ok(true)` — work was done.
    pub newly_applied: Vec<String>,
    /// Migrations that ran and returned `Ok(false)` — nothing to do.
    pub skipped_not_needed: Vec<String>,
}

// ── Public runner ─────────────────────────────────────────────────────────────

/// Run all pending migrations in order.
///
/// Migrations already in the applied set (from `migrations.json`) are skipped.
/// For each pending migration, [`Migration::run`] is called. If it returns
/// `Ok(true)` the migration name is added to the applied set and the state
/// file is written. If `run` returns an error the runner **stops immediately**
/// and propagates the error — partial progress is preserved in the state file.
pub async fn run_pending(data_dir: &Path) -> Result<MigrationRunReport, AcsError> {
    let mut applied = read_state(data_dir).await?;
    let mut report = MigrationRunReport::default();

    for migration in registry() {
        let name = migration.name().to_string();

        if applied.contains(&name) {
            report.already_applied.push(name);
            continue;
        }

        match migration.run(data_dir).await? {
            true => {
                applied.insert(name.clone());
                write_state(data_dir, &applied).await?;
                report.newly_applied.push(name);
            }
            false => {
                report.skipped_not_needed.push(name);
            }
        }
    }

    Ok(report)
}

// ── State file I/O ────────────────────────────────────────────────────────────

/// On-disk schema for `migrations.json`.
#[derive(Debug, Serialize, Deserialize, Default)]
struct MigrationState {
    applied: Vec<String>,
}

/// Read the applied-migration names from `<data_dir>/migrations.json`.
///
/// - If the file does not exist, returns an empty set (fresh install).
/// - If the file is corrupt (invalid JSON), logs a warning and returns an
///   empty set rather than failing — this prevents lockout if the state file
///   gets corrupted.
pub async fn read_state(data_dir: &Path) -> Result<HashSet<String>, AcsError> {
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
        Ok(content) => match serde_json::from_str::<MigrationState>(&content) {
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

/// Persist the applied-migration set to `<data_dir>/migrations.json`.
///
/// Uses an atomic write (write to `.tmp`, then rename) to prevent partial
/// writes from corrupting the state file.
async fn write_state(data_dir: &Path, applied: &HashSet<String>) -> Result<(), AcsError> {
    let path = data_dir.join("migrations.json");
    let tmp_path = data_dir.join("migrations.json.tmp");

    // Sort for deterministic output.
    let mut sorted: Vec<String> = applied.iter().cloned().collect();
    sorted.sort();

    let state = MigrationState { applied: sorted };
    let json = serde_json::to_string_pretty(&state)
        .map_err(|e| AcsError::Storage(format!("Failed to serialize migration state: {}", e)))?;

    tokio::fs::write(&tmp_path, json.as_bytes())
        .await
        .map_err(|e| AcsError::Storage(format!("Failed to write migrations.json.tmp: {}", e)))?;

    tokio::fs::rename(&tmp_path, &path)
        .await
        .map_err(|e| AcsError::Storage(format!("Failed to rename migrations.json.tmp: {}", e)))?;

    Ok(())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // ── Helpers ───────────────────────────────────────────────────────────────

    /// A migration that always reports it did work (applied = true).
    struct AlwaysApplies {
        name: &'static str,
    }

    #[async_trait]
    impl Migration for AlwaysApplies {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn run(&self, _data_dir: &Path) -> Result<bool, AcsError> {
            Ok(true)
        }
    }

    /// A migration that always reports nothing to do (applied = false).
    struct NeverNeeded {
        name: &'static str,
    }

    #[async_trait]
    impl Migration for NeverNeeded {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn run(&self, _data_dir: &Path) -> Result<bool, AcsError> {
            Ok(false)
        }
    }

    /// A migration that always returns an error.
    struct AlwaysFails {
        name: &'static str,
    }

    #[async_trait]
    impl Migration for AlwaysFails {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn run(&self, _data_dir: &Path) -> Result<bool, AcsError> {
            Err(AcsError::Storage("simulated migration failure".to_string()))
        }
    }

    /// Run a custom list of migrations (bypasses the real registry).
    async fn run_migrations(
        data_dir: &Path,
        migrations: Vec<Box<dyn Migration>>,
    ) -> Result<MigrationRunReport, AcsError> {
        let mut applied = read_state(data_dir).await?;
        let mut report = MigrationRunReport::default();

        for migration in migrations {
            let name = migration.name().to_string();
            if applied.contains(&name) {
                report.already_applied.push(name);
                continue;
            }
            match migration.run(data_dir).await? {
                true => {
                    applied.insert(name.clone());
                    write_state(data_dir, &applied).await?;
                    report.newly_applied.push(name);
                }
                false => {
                    report.skipped_not_needed.push(name);
                }
            }
        }
        Ok(report)
    }

    // ── State file: read/write round-trip ────────────────────────────────────

    #[tokio::test]
    async fn test_state_read_empty_when_no_file() {
        let tmp = TempDir::new().unwrap();
        let state = read_state(tmp.path()).await.unwrap();
        assert!(state.is_empty());
    }

    #[tokio::test]
    async fn test_state_write_then_read_round_trips() {
        let tmp = TempDir::new().unwrap();
        let mut set = HashSet::new();
        set.insert("m001_foo".to_string());
        set.insert("m002_bar".to_string());

        write_state(tmp.path(), &set).await.unwrap();

        let loaded = read_state(tmp.path()).await.unwrap();
        assert_eq!(loaded, set);
    }

    // ── State file: corruption tolerance ─────────────────────────────────────

    #[tokio::test]
    async fn test_run_pending_state_file_corruption_treated_as_empty() {
        let tmp = TempDir::new().unwrap();
        // Write deliberately corrupt JSON.
        tokio::fs::write(tmp.path().join("migrations.json"), b"not valid json {{{{")
            .await
            .unwrap();

        // read_state should tolerate the corruption and return an empty set.
        let state = read_state(tmp.path()).await.unwrap();
        assert!(
            state.is_empty(),
            "corrupt state file should be treated as empty"
        );
    }

    // ── Runner: all migrations run on a fresh install ─────────────────────────

    #[tokio::test]
    async fn test_run_pending_with_no_migrations_applied_runs_all() {
        let tmp = TempDir::new().unwrap();

        let report = run_migrations(
            tmp.path(),
            vec![
                Box::new(AlwaysApplies { name: "m001_a" }),
                Box::new(AlwaysApplies { name: "m002_b" }),
            ],
        )
        .await
        .unwrap();

        assert_eq!(report.newly_applied, vec!["m001_a", "m002_b"]);
        assert!(report.already_applied.is_empty());
        assert!(report.skipped_not_needed.is_empty());
    }

    // ── Runner: already-applied entries are skipped ───────────────────────────

    #[tokio::test]
    async fn test_run_pending_skips_already_applied() {
        let tmp = TempDir::new().unwrap();

        // Pre-populate the state file with m001 already applied.
        let mut pre = HashSet::new();
        pre.insert("m001_a".to_string());
        write_state(tmp.path(), &pre).await.unwrap();

        let report = run_migrations(
            tmp.path(),
            vec![
                Box::new(AlwaysApplies { name: "m001_a" }), // already applied
                Box::new(AlwaysApplies { name: "m002_b" }), // new
            ],
        )
        .await
        .unwrap();

        assert_eq!(report.already_applied, vec!["m001_a"]);
        assert_eq!(report.newly_applied, vec!["m002_b"]);
        assert!(report.skipped_not_needed.is_empty());
    }

    // ── Runner: not-needed migrations go into skipped_not_needed ─────────────

    #[tokio::test]
    async fn test_run_pending_not_needed_goes_to_skipped() {
        let tmp = TempDir::new().unwrap();

        let report = run_migrations(
            tmp.path(),
            vec![Box::new(NeverNeeded { name: "m001_noop" })],
        )
        .await
        .unwrap();

        assert!(report.newly_applied.is_empty());
        assert!(report.already_applied.is_empty());
        assert_eq!(report.skipped_not_needed, vec!["m001_noop"]);

        // A not-needed migration should NOT be written to the state file.
        let state = read_state(tmp.path()).await.unwrap();
        assert!(
            !state.contains("m001_noop"),
            "not-needed migration must not appear in state file"
        );
    }

    // ── Runner: stops at first failure ────────────────────────────────────────

    #[tokio::test]
    async fn test_run_pending_stops_at_first_failure() {
        let tmp = TempDir::new().unwrap();

        let result = run_migrations(
            tmp.path(),
            vec![
                Box::new(AlwaysApplies { name: "m001_ok" }),
                Box::new(AlwaysFails { name: "m002_fail" }),
                Box::new(AlwaysApplies { name: "m003_ok" }), // must not run
            ],
        )
        .await;

        assert!(result.is_err(), "runner should propagate the error");

        // m001 applied before the failure — must be in state.
        let state = read_state(tmp.path()).await.unwrap();
        assert!(
            state.contains("m001_ok"),
            "m001 was applied before the error"
        );
        assert!(
            !state.contains("m003_ok"),
            "m003 must not be in state (never ran)"
        );
    }

    // ── Runner: idempotent — calling twice has same result the second time ────

    #[tokio::test]
    async fn test_run_pending_idempotent() {
        let tmp = TempDir::new().unwrap();

        // First call
        let r1 = run_migrations(tmp.path(), vec![Box::new(AlwaysApplies { name: "m001_a" })])
            .await
            .unwrap();
        assert_eq!(r1.newly_applied, vec!["m001_a"]);

        // Second call with the same migration list — it is now in the applied set.
        let r2 = run_migrations(tmp.path(), vec![Box::new(AlwaysApplies { name: "m001_a" })])
            .await
            .unwrap();
        assert_eq!(r2.already_applied, vec!["m001_a"]);
        assert!(r2.newly_applied.is_empty());
    }

    // ── Atomic write — no .tmp file left behind ───────────────────────────────

    #[tokio::test]
    async fn test_run_pending_atomic_write_no_tmp_leftover() {
        let tmp = TempDir::new().unwrap();

        run_migrations(tmp.path(), vec![Box::new(AlwaysApplies { name: "m001_a" })])
            .await
            .unwrap();

        let tmp_file = tmp.path().join("migrations.json.tmp");
        assert!(
            !tmp_file.exists(),
            "migrations.json.tmp must not exist after successful write"
        );

        // The real state file must exist and be valid.
        assert!(
            tmp.path().join("migrations.json").exists(),
            "migrations.json must exist"
        );
    }

    // ── Real registry smoke test: run_pending with real data dir ─────────────

    #[tokio::test]
    async fn test_run_pending_real_registry_fresh_install() {
        // Fresh install: no jobs.json, no workflows.json, no acs.db.
        // m001 has nothing to do; m002 creates the empty SQLite DB.
        let tmp = TempDir::new().unwrap();
        let report = run_pending(tmp.path()).await.unwrap();

        assert!(report.already_applied.is_empty());
        assert!(report
            .skipped_not_needed
            .contains(&"m001_jobs_to_workflows".to_string()));
        assert!(report
            .newly_applied
            .contains(&"m002_json_to_sqlite".to_string()));
        assert!(
            tmp.path().join("acs.db").exists(),
            "m002 must have created acs.db on a fresh install"
        );
    }
}
