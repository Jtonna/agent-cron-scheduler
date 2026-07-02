//! Migration 002 — Move workflow definitions and run history from JSON files
//! into the SQLite database at `<data_dir>/acs.db`.
//!
//! # Decision table
//!
//! 1. If `<data_dir>/acs.db` contains a `workflows` table → the conversion
//!    (or the fresh-install schema) is already in place (no-op, returns
//!    `Ok(false)`). The check is table-based rather than file-based because
//!    the migration runner creates `acs.db` up front to host its
//!    `schema_migrations` tracking table, so file existence alone proves
//!    nothing.
//! 2. If neither `<data_dir>/workflows.json` nor `<data_dir>/runs/` exists →
//!    fresh install. Create `acs.db` with the empty schema and return
//!    `Ok(true)` so the migration is recorded as applied.
//! 3. Otherwise → run the full migration:
//!    a. Open `acs.db` (applies pragmas + schema), start a transaction.
//!    b. Read `workflows.json`; INSERT every workflow into `workflows`.
//!    c. Walk `runs/<workflow_id>/<run_id>.json`; INSERT every run into
//!    `workflow_runs`. `runs/index.json` is ignored — the SQLite primary
//!    keys replace it.
//!    d. Verify counts; pick one workflow (and one run if any) and re-read
//!    it from SQLite to confirm round-trip equality.
//!    e. COMMIT. Then delete `workflows.json` and the `runs/` directory.
//!
//! # Failure handling
//!
//! On any error during steps a–d the transaction is rolled back explicitly
//! and the partial `acs.db` file is deleted so re-running the migration
//! starts cleanly. The JSON sources are left untouched on failure. The error
//! is returned; `migration::run_pending` propagates it, which aborts daemon
//! startup with a non-zero exit code.
//!
//! # `migrations.json` is intentionally left in place
//!
//! Migration execution is tracked in the `schema_migrations` table inside
//! `acs.db` (owned by the migration runner, see `migration::mod`).
//! `migrations.json` is the legacy tracking file: the runner reads it one
//! time to backfill the tracking table on databases that predate it, and
//! never writes it again. This migration leaves the file untouched. The
//! `meta` table exists in the schema for future SQLite-only metadata but is
//! unused by this migration.

use std::collections::HashSet;
use std::path::Path;

use async_trait::async_trait;
use rusqlite::{params, Connection};
use uuid::Uuid;

use crate::errors::AcsError;
use crate::migration::Migration;
use crate::models::workflow::{Workflow, WorkflowRun};
use crate::storage::sqlite;

// ─── Migration impl ───────────────────────────────────────────────────────────

/// Migration 002: convert `workflows.json` + `runs/**/*.json` → `acs.db`.
pub struct JsonToSqlite;

#[async_trait]
impl Migration for JsonToSqlite {
    fn name(&self) -> &'static str {
        "m002_json_to_sqlite"
    }

    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError> {
        let db_path = data_dir.join("acs.db");
        let workflows_path = data_dir.join("workflows.json");
        let runs_dir = data_dir.join("runs");

        // 1. A `workflows` table in acs.db means the conversion (or the
        //    fresh-install schema) is already in place. Table-based rather
        //    than file-based: the migration runner creates acs.db up front to
        //    host its `schema_migrations` tracking table, so the file may
        //    exist before this migration has ever run.
        if db_path.exists() {
            let db_path_check = db_path.clone();
            let has_workflows_table =
                tokio::task::spawn_blocking(move || workflows_table_exists(&db_path_check))
                    .await
                    .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))??;
            if has_workflows_table {
                return Ok(false);
            }
        }

        // 2. Fresh install: no JSON sources to read. Create the schema-only
        //    DB and record the migration as applied.
        let has_workflows_json = workflows_path.exists();
        let has_runs_dir = runs_dir.exists();
        if !has_workflows_json && !has_runs_dir {
            // `init_db` opens, applies pragmas, applies schema. The returned
            // SqliteDb is dropped immediately — that closes the connection
            // and flushes WAL, which is what we want for a fresh install.
            let _db = sqlite::init_db(&db_path)?;
            tracing::info!(
                "m002_json_to_sqlite: fresh install — created {}",
                db_path.display()
            );
            return Ok(true);
        }

        // 3. Full migration. Read sources first so a parse failure aborts
        //    before we ever touch the DB.
        let workflows = read_workflows_json(&workflows_path).await?;
        let all_runs = read_run_files(&runs_dir).await?;

        // Filter out orphaned runs: run records whose `workflow_id` does not
        // match any workflow currently in `workflows.json`. The legacy
        // FS-backed `delete_workflow` did not cascade into `runs/`, so a
        // long-lived install can accumulate orphan run files for workflows
        // that were deleted months ago. The SQLite schema has a FOREIGN KEY
        // on `workflow_runs.workflow_id`, so attempting to insert these would
        // fail with a constraint violation. The right call is to drop them on
        // the floor — they are dead history with no live parent — and log a
        // warning so the operator can investigate if they wanted to keep the
        // run files for some out-of-band reason.
        let valid_workflow_ids: HashSet<Uuid> = workflows.iter().map(|w| w.id).collect();
        let total_run_files = all_runs.len();
        let mut runs: Vec<WorkflowRun> = Vec::with_capacity(total_run_files);
        let mut orphan_count: usize = 0;
        for run in all_runs {
            if valid_workflow_ids.contains(&run.workflow_id) {
                runs.push(run);
            } else {
                orphan_count += 1;
                tracing::warn!(
                    "Migration m002: skipping orphaned run '{}' (workflow '{}' no longer exists in workflows.json)",
                    run.run_id,
                    run.workflow_id
                );
            }
        }

        // Drive the SQLite work on a blocking task — rusqlite is sync.
        let db_path_clone = db_path.clone();
        let workflow_count = workflows.len();
        let run_count = runs.len();

        let result: Result<(), AcsError> =
            tokio::task::spawn_blocking(move || migrate_blocking(&db_path_clone, workflows, runs))
                .await
                .map_err(|e| AcsError::Internal(format!("blocking task failed: {}", e)))?;

        match result {
            Ok(()) => {
                // 4. Once the COMMIT has succeeded the data lives in SQLite
                //    and the migration is logically complete. Removing the
                //    legacy JSON sources is housekeeping — if it fails (file
                //    locked by an antivirus scanner, read-only permission,
                //    a stray editor handle, etc.) we log a warning and keep
                //    going rather than aborting daemon startup. `runs/index.json`
                //    is removed implicitly with the directory.
                if workflows_path.exists() {
                    if let Err(e) = tokio::fs::remove_file(&workflows_path).await {
                        tracing::warn!(
                            "Migration m002: SQLite commit succeeded but failed to remove {}: {}. Manual cleanup recommended.",
                            workflows_path.display(),
                            e
                        );
                    }
                }
                if runs_dir.exists() {
                    if let Err(e) = tokio::fs::remove_dir_all(&runs_dir).await {
                        tracing::warn!(
                            "Migration m002: SQLite commit succeeded but failed to remove {}: {}. Manual cleanup recommended.",
                            runs_dir.display(),
                            e
                        );
                    }
                }

                tracing::info!(
                    "Migration m002 complete: {} workflows + {} runs migrated to SQLite ({} orphaned runs skipped). Original JSON sources removed.",
                    workflow_count,
                    run_count,
                    orphan_count
                );
                tracing::debug!("Migration m002: SQLite database at {}", db_path.display());

                Ok(true)
            }
            Err(e) => {
                // Best-effort cleanup of the partial DB so a re-run starts
                // from a clean slate. The transaction was already rolled
                // back inside `migrate_blocking`; the file removal here
                // covers the case where the connection had already been
                // opened (so the file exists on disk). Removing the file also
                // drops the runner's schema_migrations rows — that is safe:
                // the runner recreates the table immediately when it records
                // this failure, and any earlier migrations simply re-run as
                // no-ops on the next startup.
                let _ = std::fs::remove_file(&db_path);
                // Also remove any WAL sidecars that may have been created.
                let _ = std::fs::remove_file(data_dir.join("acs.db-wal"));
                let _ = std::fs::remove_file(data_dir.join("acs.db-shm"));
                Err(e)
            }
        }
    }
}

/// Return `true` when `acs.db` at `path` contains a `workflows` table — the
/// marker that the conversion (or the fresh-install schema) already happened.
fn workflows_table_exists(path: &Path) -> Result<bool, AcsError> {
    let conn = Connection::open(path)
        .map_err(|e| AcsError::Storage(format!("Failed to open SQLite at {:?}: {}", path, e)))?;
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'workflows'",
            [],
            |r| r.get(0),
        )
        .map_err(|e| AcsError::Storage(format!("Failed to inspect sqlite_master: {}", e)))?;
    Ok(n > 0)
}

// ─── JSON readers ─────────────────────────────────────────────────────────────

async fn read_workflows_json(path: &Path) -> Result<Vec<Workflow>, AcsError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| AcsError::Storage(format!("Failed to read {}: {}", path.display(), e)))?;
    serde_json::from_str::<Vec<Workflow>>(&content)
        .map_err(|e| AcsError::Storage(format!("Failed to parse {}: {}", path.display(), e)))
}

/// Walk `<runs_dir>/<workflow_uuid>/<run_uuid>.json` and parse every run file.
/// Files at the top level (notably `index.json`) are skipped — the index is
/// derived metadata, not a primary source.
async fn read_run_files(runs_dir: &Path) -> Result<Vec<WorkflowRun>, AcsError> {
    if !runs_dir.exists() {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();

    let mut wf_entries = tokio::fs::read_dir(runs_dir)
        .await
        .map_err(|e| AcsError::Storage(format!("Failed to read {}: {}", runs_dir.display(), e)))?;

    while let Some(wf_entry) = wf_entries
        .next_entry()
        .await
        .map_err(|e| AcsError::Storage(e.to_string()))?
    {
        let wf_path = wf_entry.path();
        if !wf_path.is_dir() {
            // Skip `index.json` and any stray top-level files.
            continue;
        }
        let wf_id_str = wf_entry.file_name();
        let wf_id_str = wf_id_str.to_string_lossy();
        // Only descend into directories whose name parses as a UUID.
        if Uuid::parse_str(&wf_id_str).is_err() {
            continue;
        }

        let mut run_entries = tokio::fs::read_dir(&wf_path).await.map_err(|e| {
            AcsError::Storage(format!("Failed to read {}: {}", wf_path.display(), e))
        })?;
        while let Some(run_entry) = run_entries
            .next_entry()
            .await
            .map_err(|e| AcsError::Storage(e.to_string()))?
        {
            let run_path = run_entry.path();
            if !run_path.is_file() {
                continue;
            }
            let fname = run_entry.file_name();
            let fname = fname.to_string_lossy();
            // Skip `*.tmp` files (in-flight atomic writes from the legacy
            // store) and anything that isn't `.json`.
            if !fname.ends_with(".json") {
                continue;
            }
            let stem = fname.trim_end_matches(".json");
            if Uuid::parse_str(stem).is_err() {
                continue;
            }

            let content = tokio::fs::read_to_string(&run_path).await.map_err(|e| {
                AcsError::Storage(format!("Failed to read {}: {}", run_path.display(), e))
            })?;
            let run: WorkflowRun = serde_json::from_str(&content).map_err(|e| {
                AcsError::Storage(format!("Failed to parse {}: {}", run_path.display(), e))
            })?;
            runs.push(run);
        }
    }

    Ok(runs)
}

// ─── Blocking SQLite section ─────────────────────────────────────────────────

/// Synchronous half: opens the DB (pragmas + schema applied via the shared
/// `sqlite::open_with_schema` helper), runs everything inside a single
/// transaction, verifies, and commits.
fn migrate_blocking(
    db_path: &Path,
    workflows: Vec<Workflow>,
    runs: Vec<WorkflowRun>,
) -> Result<(), AcsError> {
    let mut conn = sqlite::open_with_schema(db_path)?;

    let workflow_count = workflows.len();
    let run_count = runs.len();
    let sample_workflow = workflows.first().cloned();
    let sample_run = runs.first().cloned();

    let tx = conn
        .transaction()
        .map_err(|e| AcsError::Storage(format!("Failed to start transaction: {}", e)))?;

    // Inserts. Any error returns early; `tx` is dropped → rollback.
    if let Err(e) = (|| -> Result<(), AcsError> {
        for wf in &workflows {
            insert_workflow(&tx, wf)?;
        }
        for run in &runs {
            insert_run(&tx, run)?;
        }

        // Row-count verification.
        let wf_rows: i64 = tx
            .query_row("SELECT COUNT(*) FROM workflows", [], |r| r.get(0))
            .map_err(|e| AcsError::Storage(format!("Count workflows failed: {}", e)))?;
        if wf_rows as usize != workflow_count {
            return Err(AcsError::Storage(format!(
                "workflow row count mismatch: inserted {}, found {}",
                workflow_count, wf_rows
            )));
        }
        let run_rows: i64 = tx
            .query_row("SELECT COUNT(*) FROM workflow_runs", [], |r| r.get(0))
            .map_err(|e| AcsError::Storage(format!("Count workflow_runs failed: {}", e)))?;
        if run_rows as usize != run_count {
            return Err(AcsError::Storage(format!(
                "workflow_run row count mismatch: inserted {}, found {}",
                run_count, run_rows
            )));
        }

        // Sample round-trip verification.
        if let Some(ref wf) = sample_workflow {
            verify_workflow_round_trip(&tx, wf)?;
        }
        if let Some(ref run) = sample_run {
            verify_run_round_trip(&tx, run)?;
        }

        Ok(())
    })() {
        // Explicit rollback so the failure surface is unambiguous.
        if let Err(rb_err) = tx.rollback() {
            tracing::warn!(
                "m002_json_to_sqlite: rollback after failure also failed: {}",
                rb_err
            );
        }
        return Err(e);
    }

    tx.commit()
        .map_err(|e| AcsError::Storage(format!("COMMIT failed: {}", e)))?;
    Ok(())
}

/// Insert a single workflow row. Mirrors the column layout the
/// `SqliteWorkflowStore` writes so the runtime can read these rows back
/// unchanged after the migration.
fn insert_workflow(conn: &Connection, wf: &Workflow) -> Result<(), AcsError> {
    let steps_json =
        serde_json::to_string(&wf.steps).map_err(|e| AcsError::Storage(e.to_string()))?;
    let default_input = wf
        .default_input
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AcsError::Storage(e.to_string()))?;
    let env_vars = wf
        .env_vars
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AcsError::Storage(e.to_string()))?;

    let schedule_mode = match serde_json::to_value(&wf.schedule_mode)
        .map_err(|e| AcsError::Storage(e.to_string()))?
    {
        serde_json::Value::String(s) => s,
        other => {
            return Err(AcsError::Storage(format!(
                "ScheduleMode did not serialize to a string: {}",
                other
            )));
        }
    };

    let on_failure =
        serde_json::to_string(&wf.on_failure).map_err(|e| AcsError::Storage(e.to_string()))?;

    let last_run_status = match wf.last_run_status {
        None => None,
        Some(ref s) => {
            match serde_json::to_value(s).map_err(|e| AcsError::Storage(e.to_string()))? {
                serde_json::Value::String(s) => Some(s),
                other => {
                    return Err(AcsError::Storage(format!(
                        "RunStatus did not serialize to a string: {}",
                        other
                    )));
                }
            }
        }
    };

    conn.execute(
        "INSERT INTO workflows (
            id, name, version, schedule, timezone, schedule_mode, enabled,
            steps_json, default_input, working_dir, env_vars,
            allow_concurrent, on_failure, last_run_at, last_run_status,
            last_run_id, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18
        )",
        params![
            wf.id.to_string(),
            wf.name,
            wf.version as i64,
            wf.schedule,
            wf.timezone,
            schedule_mode,
            wf.enabled as i64,
            steps_json,
            default_input,
            wf.working_dir,
            env_vars,
            wf.allow_concurrent as i64,
            on_failure,
            wf.last_run_at.map(|d| d.to_rfc3339()),
            last_run_status,
            wf.last_run_id.map(|u| u.to_string()),
            wf.created_at.to_rfc3339(),
            wf.updated_at.to_rfc3339(),
        ],
    )
    .map_err(|e| {
        AcsError::Storage(format!(
            "INSERT workflow '{}' (id={}) failed: {}",
            wf.name, wf.id, e
        ))
    })?;

    Ok(())
}

/// Insert a single workflow_run row.
fn insert_run(conn: &Connection, run: &WorkflowRun) -> Result<(), AcsError> {
    let snapshot_json = serde_json::to_string(&run.workflow_snapshot)
        .map_err(|e| AcsError::Storage(e.to_string()))?;
    let steps_json =
        serde_json::to_string(&run.steps).map_err(|e| AcsError::Storage(e.to_string()))?;
    let trigger_input = run
        .trigger_input
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| AcsError::Storage(e.to_string()))?;

    let status =
        match serde_json::to_value(run.status).map_err(|e| AcsError::Storage(e.to_string()))? {
            serde_json::Value::String(s) => s,
            other => {
                return Err(AcsError::Storage(format!(
                    "RunStatus did not serialize to a string: {}",
                    other
                )));
            }
        };

    conn.execute(
        "INSERT INTO workflow_runs (
            run_id, workflow_id, workflow_version, workflow_snapshot,
            started_at, finished_at, status, trigger_input, steps_json,
            total_cost_usd, total_duration_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
        params![
            run.run_id.to_string(),
            run.workflow_id.to_string(),
            run.workflow_version as i64,
            snapshot_json,
            run.started_at.to_rfc3339(),
            run.finished_at.map(|d| d.to_rfc3339()),
            status,
            trigger_input,
            steps_json,
            run.total_cost_usd,
            run.total_duration_ms.map(|n| n as i64),
        ],
    )
    .map_err(|e| {
        AcsError::Storage(format!(
            "INSERT workflow_run '{}' (workflow_id={}) failed: {}",
            run.run_id, run.workflow_id, e
        ))
    })?;

    Ok(())
}

/// Re-read the just-inserted workflow row as JSON-shaped fields, deserialise
/// it back into a `Workflow`, and confirm structural equality with the
/// source. Verifies the round-trip without coupling to the runtime row
/// mapper (which is private to the SQLite store module).
fn verify_workflow_round_trip(conn: &Connection, source: &Workflow) -> Result<(), AcsError> {
    let id_s = source.id.to_string();

    let (steps_json, on_failure_json, snapshot_name): (String, String, String) = conn
        .query_row(
            "SELECT steps_json, on_failure, name FROM workflows WHERE id = ?1",
            [&id_s],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|e| {
            AcsError::Storage(format!(
                "Verification SELECT for workflow {} failed: {}",
                source.id, e
            ))
        })?;

    if snapshot_name != source.name {
        return Err(AcsError::Storage(format!(
            "Workflow {} verification mismatch: name {:?} != {:?}",
            source.id, snapshot_name, source.name
        )));
    }

    let round_tripped_steps: Vec<crate::models::workflow::StepDef> =
        serde_json::from_str(&steps_json).map_err(|e| {
            AcsError::Storage(format!(
                "Verification: round-trip parse of steps for {} failed: {}",
                source.id, e
            ))
        })?;
    if round_tripped_steps != source.steps {
        return Err(AcsError::Storage(format!(
            "Workflow {} verification mismatch: steps differ after round-trip",
            source.id
        )));
    }

    let round_tripped_failure: crate::models::workflow::FailurePolicy =
        serde_json::from_str(&on_failure_json).map_err(|e| {
            AcsError::Storage(format!(
                "Verification: round-trip parse of on_failure for {} failed: {}",
                source.id, e
            ))
        })?;
    if round_tripped_failure != source.on_failure {
        return Err(AcsError::Storage(format!(
            "Workflow {} verification mismatch: on_failure differs after round-trip",
            source.id
        )));
    }

    Ok(())
}

/// Re-read the just-inserted workflow_run row, deserialise the `steps_json`
/// blob, and confirm it matches the source. Lightweight smoke check that the
/// transaction wrote what we expected.
fn verify_run_round_trip(conn: &Connection, source: &WorkflowRun) -> Result<(), AcsError> {
    let id_s = source.run_id.to_string();
    let (status_s, steps_json): (String, String) = conn
        .query_row(
            "SELECT status, steps_json FROM workflow_runs WHERE run_id = ?1",
            [&id_s],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| {
            AcsError::Storage(format!(
                "Verification SELECT for run {} failed: {}",
                source.run_id, e
            ))
        })?;

    let expected_status =
        match serde_json::to_value(source.status).map_err(|e| AcsError::Storage(e.to_string()))? {
            serde_json::Value::String(s) => s,
            other => {
                return Err(AcsError::Storage(format!(
                    "RunStatus did not serialize to a string: {}",
                    other
                )));
            }
        };
    if status_s != expected_status {
        return Err(AcsError::Storage(format!(
            "Run {} verification mismatch: status {:?} != {:?}",
            source.run_id, status_s, expected_status
        )));
    }

    let round_tripped_steps: Vec<crate::models::workflow::StepRun> =
        serde_json::from_str(&steps_json).map_err(|e| {
            AcsError::Storage(format!(
                "Verification: round-trip parse of steps for run {} failed: {}",
                source.run_id, e
            ))
        })?;
    if round_tripped_steps != source.steps {
        return Err(AcsError::Storage(format!(
            "Run {} verification mismatch: steps differ after round-trip",
            source.run_id
        )));
    }

    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, NewWorkflow, RunStatus, ScheduleMode, ShellStep, StepDef,
        StepDefCommon, Workflow, WorkflowRun,
    };
    use chrono::Utc;
    use std::collections::HashMap;
    use tempfile::TempDir;

    fn make_shell_step(id: &str) -> StepDef {
        StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(30),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: "echo hello".to_string(),
            pass_stdin: false,
        })
    }

    fn make_workflow(name: &str) -> Workflow {
        let now = Utc::now();
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        Workflow {
            id: Uuid::now_v7(),
            name: name.to_string(),
            version: 1,
            schedule: "*/5 * * * *".to_string(),
            timezone: Some("America/New_York".to_string()),
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            is_favorited: false,
            deleted: false,
            steps: vec![make_shell_step("step-1")],
            default_input: Some(serde_json::json!({"k": 1})),
            working_dir: Some("/tmp".to_string()),
            env_vars: Some(env),
            allow_concurrent: true,
            on_failure: FailurePolicy::default(),
            last_run_at: None,
            last_run_status: None,
            last_run_id: None,
            next_run_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn make_run(parent: &Workflow) -> WorkflowRun {
        WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: parent.id,
            workflow_version: parent.version,
            workflow_snapshot: parent.clone(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    async fn write_workflows_json(dir: &Path, workflows: &[Workflow]) {
        let json = serde_json::to_string_pretty(workflows).unwrap();
        tokio::fs::write(dir.join("workflows.json"), json.as_bytes())
            .await
            .unwrap();
    }

    async fn write_run_json(dir: &Path, run: &WorkflowRun) {
        let runs_dir = dir.join("runs").join(run.workflow_id.to_string());
        tokio::fs::create_dir_all(&runs_dir).await.unwrap();
        let json = serde_json::to_string_pretty(run).unwrap();
        tokio::fs::write(
            runs_dir.join(format!("{}.json", run.run_id)),
            json.as_bytes(),
        )
        .await
        .unwrap();
    }

    fn count_rows(db_path: &Path, table: &str) -> i64 {
        // Re-uses the shared opener; `apply_schema` is idempotent so this is
        // a read-only-feeling helper despite running the schema DDL.
        let conn = sqlite::open_with_schema(db_path).expect("open verify");
        conn.query_row(&format!("SELECT COUNT(*) FROM {}", table), [], |r| r.get(0))
            .expect("count")
    }

    // ── 1. Fresh install: no JSON sources, no runs/ ──────────────────────────

    #[tokio::test]
    async fn test_fresh_install_creates_empty_db() {
        let tmp = TempDir::new().unwrap();
        let m = JsonToSqlite;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(applied, "fresh install should report Ok(true)");

        let db_path = tmp.path().join("acs.db");
        assert!(db_path.exists(), "acs.db must exist after fresh install");
        assert_eq!(count_rows(&db_path, "workflows"), 0);
        assert_eq!(count_rows(&db_path, "workflow_runs"), 0);
    }

    // ── 2. Migrate workflows + runs ──────────────────────────────────────────

    #[tokio::test]
    async fn test_migrate_workflows_and_runs() {
        let tmp = TempDir::new().unwrap();
        let wf_a = make_workflow("a");
        let wf_b = make_workflow("b");
        write_workflows_json(tmp.path(), &[wf_a.clone(), wf_b.clone()]).await;

        let run_a = make_run(&wf_a);
        let run_b = make_run(&wf_b);
        write_run_json(tmp.path(), &run_a).await;
        write_run_json(tmp.path(), &run_b).await;

        let m = JsonToSqlite;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(applied, "migration with sources should return Ok(true)");

        // SQLite contents.
        let db_path = tmp.path().join("acs.db");
        assert_eq!(count_rows(&db_path, "workflows"), 2);
        assert_eq!(count_rows(&db_path, "workflow_runs"), 2);

        // JSON sources removed.
        assert!(
            !tmp.path().join("workflows.json").exists(),
            "workflows.json must be removed after successful commit"
        );
        assert!(
            !tmp.path().join("runs").exists(),
            "runs/ directory must be removed after successful commit"
        );
    }

    // ── 3. Idempotency: acs.db already exists → Ok(false), no DB changes ────

    #[tokio::test]
    async fn test_idempotent_when_db_already_exists() {
        let tmp = TempDir::new().unwrap();
        // Pre-create acs.db.
        let _ = sqlite::init_db(&tmp.path().join("acs.db")).unwrap();
        let mtime_before = std::fs::metadata(tmp.path().join("acs.db"))
            .unwrap()
            .modified()
            .unwrap();

        // Even with JSON sources lying around, second call must be a no-op.
        let wf = make_workflow("noop");
        write_workflows_json(tmp.path(), &[wf]).await;

        let m = JsonToSqlite;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(!applied, "second call must return Ok(false)");

        // DB file untouched (mtime unchanged).
        let mtime_after = std::fs::metadata(tmp.path().join("acs.db"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(mtime_before, mtime_after);

        // workflows.json must still be on disk — migration didn't run.
        assert!(tmp.path().join("workflows.json").exists());
    }

    // ── 4. Corrupt workflows.json → Err, db deleted, sources untouched ──────

    #[tokio::test]
    async fn test_corrupt_workflows_json_rolls_back() {
        let tmp = TempDir::new().unwrap();
        tokio::fs::write(tmp.path().join("workflows.json"), b"not valid json{{{")
            .await
            .unwrap();

        let m = JsonToSqlite;
        let result = m.run(tmp.path()).await;
        assert!(result.is_err(), "corrupt JSON must fail");

        assert!(
            !tmp.path().join("acs.db").exists(),
            "acs.db must be removed on rollback"
        );
        assert!(
            tmp.path().join("workflows.json").exists(),
            "workflows.json must NOT be touched on failure"
        );
    }

    // ── 5. Mid-run failure: malformed run file → Err, db gone, sources intact

    #[tokio::test]
    async fn test_corrupt_run_file_rolls_back() {
        let tmp = TempDir::new().unwrap();
        let wf = make_workflow("with-bad-run");
        write_workflows_json(tmp.path(), &[wf.clone()]).await;

        // Create a runs/<wf>/ dir with a malformed JSON file.
        let runs_dir = tmp.path().join("runs").join(wf.id.to_string());
        tokio::fs::create_dir_all(&runs_dir).await.unwrap();
        let bogus_run = format!("{}.json", Uuid::now_v7());
        tokio::fs::write(runs_dir.join(&bogus_run), b"{{{not valid")
            .await
            .unwrap();

        let m = JsonToSqlite;
        let result = m.run(tmp.path()).await;
        assert!(result.is_err(), "malformed run file must fail migration");

        assert!(
            !tmp.path().join("acs.db").exists(),
            "acs.db must be removed on rollback"
        );
        assert!(
            tmp.path().join("workflows.json").exists(),
            "workflows.json must NOT be touched on failure"
        );
        assert!(
            tmp.path().join("runs").exists(),
            "runs/ must NOT be touched on failure"
        );
        assert!(
            runs_dir.join(&bogus_run).exists(),
            "the malformed run file itself must remain on disk"
        );
    }

    // ── 6. migrations.json is left alone ─────────────────────────────────────

    #[tokio::test]
    async fn test_migrations_json_untouched() {
        let tmp = TempDir::new().unwrap();
        let migrations_content = r#"{"applied":["m001_jobs_to_workflows"]}"#;
        tokio::fs::write(tmp.path().join("migrations.json"), migrations_content)
            .await
            .unwrap();

        // Empty data dir otherwise → fresh-install branch, but migrations.json
        // must still be there afterwards.
        let m = JsonToSqlite;
        m.run(tmp.path()).await.unwrap();

        assert!(tmp.path().join("migrations.json").exists());
        let on_disk = tokio::fs::read_to_string(tmp.path().join("migrations.json"))
            .await
            .unwrap();
        assert_eq!(on_disk, migrations_content);
    }

    // ── 7. index.json inside runs/ is ignored on read and removed on commit ─

    #[tokio::test]
    async fn test_runs_index_json_is_ignored_and_removed() {
        let tmp = TempDir::new().unwrap();
        let wf = make_workflow("indexed");
        write_workflows_json(tmp.path(), &[wf.clone()]).await;
        let run = make_run(&wf);
        write_run_json(tmp.path(), &run).await;

        // Stage an index.json sibling of the per-workflow run dirs.
        let runs_dir = tmp.path().join("runs");
        tokio::fs::write(runs_dir.join("index.json"), b"{}")
            .await
            .unwrap();

        let m = JsonToSqlite;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(applied);

        // Migration should have ignored index.json (no parse error from it)
        // and then deleted the entire runs/ dir on success.
        assert!(!runs_dir.exists(), "runs/ should have been removed");

        // Only the legitimate run is in the DB.
        assert_eq!(
            count_rows(&tmp.path().join("acs.db"), "workflow_runs"),
            1,
            "only one run record should be inserted"
        );
    }

    // ── 8. Orphan run files are skipped instead of failing the migration ────
    //
    // Regression test for v4.2.0 smoke-test failure: the legacy FS-backed
    // `delete_workflow` did not cascade into `runs/`, so installs accumulated
    // run files for workflows that no longer exist. The new SQLite schema
    // has a FOREIGN KEY on `workflow_runs.workflow_id`; without filtering,
    // those orphan rows would trip the constraint and abort the migration.

    #[tokio::test]
    async fn test_orphan_run_files_are_skipped() {
        let tmp = TempDir::new().unwrap();

        // One live workflow with one valid run.
        let wf = make_workflow("alive");
        write_workflows_json(tmp.path(), &[wf.clone()]).await;
        let valid_run = make_run(&wf);
        write_run_json(tmp.path(), &valid_run).await;

        // An orphan: a run whose `workflow_id` is NOT in workflows.json.
        // We synthesise a workflow only to build a plausible run snapshot;
        // the snapshot's id is what gates the FK check.
        let ghost_wf = make_workflow("ghost-deleted");
        let orphan_run = make_run(&ghost_wf);
        write_run_json(tmp.path(), &orphan_run).await;

        // Sanity: both run files exist before migration.
        let runs_dir = tmp.path().join("runs");
        let valid_run_file = runs_dir
            .join(valid_run.workflow_id.to_string())
            .join(format!("{}.json", valid_run.run_id));
        let orphan_run_file = runs_dir
            .join(orphan_run.workflow_id.to_string())
            .join(format!("{}.json", orphan_run.run_id));
        assert!(valid_run_file.exists());
        assert!(orphan_run_file.exists());

        let m = JsonToSqlite;
        let applied = m
            .run(tmp.path())
            .await
            .expect("migration must succeed despite orphan run files");
        assert!(applied);

        let db_path = tmp.path().join("acs.db");
        assert_eq!(
            count_rows(&db_path, "workflows"),
            1,
            "only the live workflow is in SQLite"
        );
        assert_eq!(
            count_rows(&db_path, "workflow_runs"),
            1,
            "only the run for the live workflow is in SQLite (orphan was skipped)"
        );

        // The whole runs/ directory is removed on successful commit, so both
        // run files are gone after migration. That includes the orphan — it
        // is dead history with no live parent in either place.
        assert!(
            !runs_dir.exists(),
            "runs/ should be removed wholesale after a successful migration"
        );
    }

    // ── 9. Post-COMMIT cleanup-failure path is non-fatal ─────────────────────
    //
    // Windows-only because it relies on the OS denying `remove_dir_all` while
    // an open file handle exists inside the directory. POSIX happily unlinks
    // files (and rmdirs empty dirs) even with open handles, so the same
    // failure mode is not reliably reproducible on Unix in a portable test.
    // The warn-and-continue branch itself is platform-agnostic.

    #[cfg(windows)]
    #[tokio::test]
    async fn test_cleanup_failure_is_non_fatal() {
        use std::fs::OpenOptions;
        use std::os::windows::fs::OpenOptionsExt;

        let tmp = TempDir::new().unwrap();
        let wf = make_workflow("cleanup-fail");
        write_workflows_json(tmp.path(), &[wf.clone()]).await;
        let run = make_run(&wf);
        write_run_json(tmp.path(), &run).await;

        // Open the run JSON with a restrictive share mode (no FILE_SHARE_DELETE)
        // so Windows refuses any unlink attempt while this handle lives.
        // FILE_SHARE_READ = 0x1, FILE_SHARE_WRITE = 0x2; deliberately omit
        // FILE_SHARE_DELETE = 0x4.
        let runs_dir = tmp.path().join("runs");
        let run_file = runs_dir
            .join(run.workflow_id.to_string())
            .join(format!("{}.json", run.run_id));
        let _locked = OpenOptions::new()
            .read(true)
            .share_mode(0x1 | 0x2)
            .open(&run_file)
            .expect("open run file with deny-delete sharing");

        // Migration must still succeed — the SQLite COMMIT happens before
        // the cleanup, so locked-file aftermath is just a logged warning.
        let m = JsonToSqlite;
        let applied = m
            .run(tmp.path())
            .await
            .expect("migration must succeed even when cleanup fails");
        assert!(applied);

        // acs.db is populated.
        let db_path = tmp.path().join("acs.db");
        assert!(
            db_path.exists(),
            "acs.db must exist after successful commit"
        );
        assert_eq!(count_rows(&db_path, "workflows"), 1);
        assert_eq!(count_rows(&db_path, "workflow_runs"), 1);

        // The locked file (and therefore its parent runs/ tree) is still on
        // disk because cleanup failed. The migration tolerated that and
        // returned Ok(true).
        assert!(
            run_file.exists(),
            "locked run file remained on disk; cleanup correctly skipped it"
        );
        assert!(runs_dir.exists(), "runs/ remained on disk");
    }

    // ── 10. Compatibility with NewWorkflow shape via SqliteWorkflowStore ────

    #[tokio::test]
    async fn test_migrated_rows_readable_by_sqlite_store() {
        // Ensure the row layout is wire-compatible with the runtime store
        // by reading the DB through SqliteWorkflowStore after the migration.
        use crate::storage::sqlite::SqliteWorkflowStore;
        use crate::storage::workflows::WorkflowStore;

        let tmp = TempDir::new().unwrap();
        let wf = make_workflow("readable");
        write_workflows_json(tmp.path(), &[wf.clone()]).await;

        let m = JsonToSqlite;
        m.run(tmp.path()).await.unwrap();

        let db = sqlite::init_db(&tmp.path().join("acs.db")).unwrap();
        let store = SqliteWorkflowStore::new(&db);
        let listed = store.list_workflows().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, wf.id);
        assert_eq!(listed[0].name, wf.name);
        assert_eq!(listed[0].steps, wf.steps);

        // Smoke-check: NewWorkflow type is still available (referenced from
        // tests so the compiler keeps it imported even if other tests drop).
        let _: NewWorkflow = NewWorkflow {
            name: "x".into(),
            schedule: "* * * * *".into(),
            timezone: None,
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            steps: vec![make_shell_step("s")],
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: None,
            on_failure: FailurePolicy::default(),
        };
    }
}
