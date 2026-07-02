//! SQLite implementation of [`WorkflowStore`].

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::{
    validate_new_workflow, validate_workflow_update, FailurePolicy, NewWorkflow, RunStatus,
    ScheduleMode, StepDef, Workflow, WorkflowUpdate,
};
use crate::storage::sqlite::row_helpers::{
    map_serde_err, map_uuid_err, parse_dt, parse_opt_dt, run_status_str,
};
use crate::storage::sqlite::SqliteDb;
use crate::storage::workflows::WorkflowStore;

/// SQLite-backed implementation of [`WorkflowStore`].
pub struct SqliteWorkflowStore {
    db: SqliteDb,
}

impl SqliteWorkflowStore {
    /// Construct from a shared [`SqliteDb`]. The schema must already have been
    /// applied (call [`crate::storage::sqlite::init_db`] first).
    pub fn new(db: &SqliteDb) -> Self {
        Self { db: db.clone() }
    }

    /// Build an isolated in-memory store for unit tests.
    #[cfg(test)]
    fn in_memory_for_tests() -> Self {
        let db = crate::storage::sqlite::init_in_memory_db().expect("init in-memory db");
        Self::new(&db)
    }
}

// ─── Row mapping ─────────────────────────────────────────────────────────────

fn row_to_workflow(row: &Row<'_>) -> rusqlite::Result<Workflow> {
    let id_s: String = row.get("id")?;
    let id = Uuid::parse_str(&id_s).map_err(map_uuid_err)?;
    let steps_json: String = row.get("steps_json")?;
    let steps: Vec<StepDef> = serde_json::from_str(&steps_json).map_err(map_serde_err)?;

    let timezone: Option<String> = row.get("timezone")?;
    let schedule_mode_s: String = row.get("schedule_mode")?;
    let schedule_mode: ScheduleMode =
        serde_json::from_value(serde_json::Value::String(schedule_mode_s))
            .map_err(map_serde_err)?;

    let on_failure_json: String = row.get("on_failure")?;
    let on_failure: FailurePolicy =
        serde_json::from_str(&on_failure_json).map_err(map_serde_err)?;

    let default_input_s: Option<String> = row.get("default_input")?;
    let default_input = match default_input_s {
        Some(s) => Some(serde_json::from_str::<serde_json::Value>(&s).map_err(map_serde_err)?),
        None => None,
    };
    let env_vars_s: Option<String> = row.get("env_vars")?;
    let env_vars = match env_vars_s {
        Some(s) => Some(serde_json::from_str(&s).map_err(map_serde_err)?),
        None => None,
    };

    let last_run_at = parse_opt_dt(row, "last_run_at")?;
    let last_run_status_s: Option<String> = row.get("last_run_status")?;
    let last_run_status = match last_run_status_s {
        Some(s) => Some(
            serde_json::from_value::<RunStatus>(serde_json::Value::String(s))
                .map_err(map_serde_err)?,
        ),
        None => None,
    };
    let last_run_id_s: Option<String> = row.get("last_run_id")?;
    let last_run_id = match last_run_id_s {
        Some(s) => Some(Uuid::parse_str(&s).map_err(map_uuid_err)?),
        None => None,
    };

    let created_at = parse_dt(row, "created_at")?;
    let updated_at = parse_dt(row, "updated_at")?;

    // is_favorited was added in a later schema migration. Tolerate older rows
    // that don't have the column populated by treating a missing column as false.
    let is_favorited = row.get::<_, i64>("is_favorited").unwrap_or(0) != 0;
    // `deleted` (soft-delete, ACS-25) — tolerate DBs where the column has not
    // yet been added by migration m008 (treat missing as not-deleted).
    let deleted = row.get::<_, i64>("deleted").unwrap_or(0) != 0;

    Ok(Workflow {
        id,
        name: row.get("name")?,
        version: row.get::<_, i64>("version")? as u32,
        schedule: row.get("schedule")?,
        timezone,
        schedule_mode,
        enabled: row.get::<_, i64>("enabled")? != 0,
        is_favorited,
        deleted,
        steps,
        default_input,
        working_dir: row.get("working_dir")?,
        env_vars,
        allow_concurrent: row.get::<_, i64>("allow_concurrent")? != 0,
        on_failure,
        last_run_at,
        last_run_status,
        last_run_id,
        next_run_at: None,
        created_at,
        updated_at,
    })
}

/// Serialize a `ScheduleMode` to its bare snake_case string (e.g. "cron").
fn schedule_mode_str(m: &ScheduleMode) -> Result<String, AcsError> {
    match serde_json::to_value(m).map_err(|e| AcsError::Storage(e.to_string()))? {
        serde_json::Value::String(s) => Ok(s),
        other => Err(AcsError::Storage(format!(
            "ScheduleMode did not serialize to a string: {}",
            other
        ))),
    }
}

/// Serialize a `FailurePolicy` to a TEXT blob. Unlike `ScheduleMode` /
/// `RunStatus`, `FailurePolicy::Retry { .. }` carries data — so we always
/// store the full serde_json representation, not a bare string.
fn on_failure_blob(p: &FailurePolicy) -> Result<String, AcsError> {
    serde_json::to_string(p).map_err(|e| AcsError::Storage(e.to_string()))
}

/// Map a rusqlite error during INSERT to an `AcsError`. Specifically detect a
/// constraint violation — in practice the partial unique index
/// `idx_workflows_name_live` on `workflows(name) WHERE deleted = 0` — and
/// translate it to `Conflict` (surfaced as HTTP 409).
fn map_insert_err(name: &str, e: rusqlite::Error) -> AcsError {
    if let rusqlite::Error::SqliteFailure(ref err, _) = e {
        if err.code == rusqlite::ErrorCode::ConstraintViolation {
            return AcsError::Conflict(format!("A workflow with name '{}' already exists", name));
        }
    }
    AcsError::Storage(format!("INSERT workflow failed: {}", e))
}

// ─── Trait impl ───────────────────────────────────────────────────────────────

#[async_trait]
impl WorkflowStore for SqliteWorkflowStore {
    async fn list_workflows(&self) -> Result<Vec<Workflow>> {
        let res = self
            .db
            .with_conn(|c| {
                // Exclude soft-deleted rows (ACS-25).
                let mut stmt = c
                    .prepare("SELECT * FROM workflows WHERE deleted = 0 ORDER BY created_at ASC")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map([], row_to_workflow)
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| AcsError::Storage(e.to_string()))?);
                }
                Ok(out)
            })
            .await?;
        Ok(res)
    }

    async fn list_workflows_including_deleted(&self) -> Result<Vec<Workflow>> {
        let res = self
            .db
            .with_conn(|c| {
                let mut stmt = c
                    .prepare("SELECT * FROM workflows ORDER BY created_at ASC")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                let rows = stmt
                    .query_map([], row_to_workflow)
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                let mut out = Vec::new();
                for r in rows {
                    out.push(r.map_err(|e| AcsError::Storage(e.to_string()))?);
                }
                Ok(out)
            })
            .await?;
        Ok(res)
    }

    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>> {
        let id_s = id.to_string();
        let res = self
            .db
            .with_conn(move |c| {
                // Exclude soft-deleted rows (ACS-25): a deleted workflow is
                // "not found" for the normal endpoints.
                let mut stmt = c
                    .prepare("SELECT * FROM workflows WHERE id = ? AND deleted = 0")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                stmt.query_row([&id_s], row_to_workflow)
                    .optional()
                    .map_err(|e| AcsError::Storage(e.to_string()))
            })
            .await?;
        Ok(res)
    }

    async fn get_workflow_including_deleted(&self, id: Uuid) -> Result<Option<Workflow>> {
        let id_s = id.to_string();
        let res = self
            .db
            .with_conn(move |c| {
                let mut stmt = c
                    .prepare("SELECT * FROM workflows WHERE id = ?")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                stmt.query_row([&id_s], row_to_workflow)
                    .optional()
                    .map_err(|e| AcsError::Storage(e.to_string()))
            })
            .await?;
        Ok(res)
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>> {
        let name = name.to_string();
        let res = self
            .db
            .with_conn(move |c| {
                // Exclude soft-deleted rows so a reused name resolves to the
                // live workflow (ACS-25).
                let mut stmt = c
                    .prepare("SELECT * FROM workflows WHERE name = ? AND deleted = 0")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                stmt.query_row([&name], row_to_workflow)
                    .optional()
                    .map_err(|e| AcsError::Storage(e.to_string()))
            })
            .await?;
        Ok(res)
    }

    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow> {
        validate_new_workflow(&new)?;

        let now = Utc::now();
        let workflow = Workflow {
            id: Uuid::now_v7(),
            name: new.name,
            version: 1,
            schedule: new.schedule,
            timezone: new.timezone,
            schedule_mode: new.schedule_mode,
            enabled: new.enabled,
            is_favorited: false,
            deleted: false,
            steps: new.steps,
            default_input: new.default_input,
            working_dir: new.working_dir,
            env_vars: new.env_vars,
            allow_concurrent: new.allow_concurrent.unwrap_or(true),
            on_failure: new.on_failure,
            last_run_at: None,
            last_run_status: None,
            last_run_id: None,
            next_run_at: None,
            created_at: now,
            updated_at: now,
        };

        let wf_clone = workflow.clone();
        self.db
            .with_conn(move |c| {
                insert_workflow(c, &wf_clone)?;
                Ok(())
            })
            .await?;

        Ok(workflow)
    }

    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow> {
        validate_workflow_update(&update)?;

        let res: Workflow = self
            .db
            .with_conn(move |c| {
                // Fetch existing.
                let mut wf = {
                    // Soft-deleted workflows are "not found" for updates (ACS-25).
                    let mut stmt = c
                        .prepare("SELECT * FROM workflows WHERE id = ? AND deleted = 0")
                        .map_err(|e| AcsError::Storage(e.to_string()))?;
                    stmt.query_row([&id.to_string()], row_to_workflow)
                        .optional()
                        .map_err(|e| AcsError::Storage(e.to_string()))?
                        .ok_or_else(|| {
                            AcsError::NotFound(format!("Workflow with id '{}' not found", id))
                        })?
                };

                // Duplicate-name check, excluding self and soft-deleted rows.
                if let Some(ref new_name) = update.name {
                    let count: i64 = c
                        .query_row(
                            "SELECT COUNT(*) FROM workflows \
                             WHERE name = ? AND id != ? AND deleted = 0",
                            params![new_name, id.to_string()],
                            |r| r.get(0),
                        )
                        .map_err(|e| AcsError::Storage(e.to_string()))?;
                    if count > 0 {
                        return Err(AcsError::Conflict(format!(
                            "A workflow with name '{}' already exists",
                            new_name
                        )));
                    }
                }

                // Bump version on any definition-affecting field change;
                // `enabled` does NOT bump.
                let mut definition_changed = false;

                if let Some(name) = update.name {
                    if wf.name != name {
                        definition_changed = true;
                    }
                    wf.name = name;
                }
                if let Some(schedule) = update.schedule {
                    if wf.schedule != schedule {
                        definition_changed = true;
                    }
                    wf.schedule = schedule;
                }
                if let Some(timezone) = update.timezone {
                    let new_tz = Some(timezone);
                    if wf.timezone != new_tz {
                        definition_changed = true;
                    }
                    wf.timezone = new_tz;
                }
                if let Some(schedule_mode) = update.schedule_mode {
                    if wf.schedule_mode != schedule_mode {
                        definition_changed = true;
                    }
                    wf.schedule_mode = schedule_mode;
                }
                if let Some(enabled) = update.enabled {
                    wf.enabled = enabled;
                }
                if let Some(is_favorited) = update.is_favorited {
                    // is_favorited is UI-only metadata; never bumps version.
                    wf.is_favorited = is_favorited;
                }
                if let Some(steps) = update.steps {
                    if wf.steps != steps {
                        definition_changed = true;
                    }
                    wf.steps = steps;
                }
                if let Some(default_input) = update.default_input {
                    if wf.default_input.as_ref() != Some(&default_input) {
                        definition_changed = true;
                    }
                    wf.default_input = Some(default_input);
                }
                if let Some(working_dir) = update.working_dir {
                    let new_wd = Some(working_dir);
                    if wf.working_dir != new_wd {
                        definition_changed = true;
                    }
                    wf.working_dir = new_wd;
                }
                if let Some(env_vars) = update.env_vars {
                    if wf.env_vars.as_ref() != Some(&env_vars) {
                        definition_changed = true;
                    }
                    wf.env_vars = Some(env_vars);
                }
                if let Some(allow_concurrent) = update.allow_concurrent {
                    if wf.allow_concurrent != allow_concurrent {
                        definition_changed = true;
                    }
                    wf.allow_concurrent = allow_concurrent;
                }
                if let Some(on_failure) = update.on_failure {
                    if wf.on_failure != on_failure {
                        definition_changed = true;
                    }
                    wf.on_failure = on_failure;
                }

                if definition_changed {
                    wf.version += 1;
                }
                wf.updated_at = Utc::now();

                update_workflow_row(c, &wf)?;
                Ok(wf)
            })
            .await?;

        Ok(res)
    }

    async fn delete_workflow(&self, id: Uuid) -> Result<()> {
        let now_s = Utc::now().to_rfc3339();
        self.db
            .with_conn(move |c| {
                // Soft delete (ACS-25): flag the row, keep it and every
                // workflow_runs record, and keep the name verbatim — the
                // partial unique index `idx_workflows_name_live` only covers
                // rows with deleted = 0, so the name is freed for reuse.
                let n = c
                    .execute(
                        "UPDATE workflows SET deleted = 1, updated_at = ?1 \
                         WHERE id = ?2 AND deleted = 0",
                        params![now_s, id.to_string()],
                    )
                    .map_err(|e| AcsError::Storage(format!("soft-delete UPDATE failed: {}", e)))?;
                if n == 0 {
                    return Err(AcsError::NotFound(format!(
                        "Workflow with id '{}' not found",
                        id
                    )));
                }
                Ok(())
            })
            .await?;
        Ok(())
    }

    async fn set_favorited(&self, id: Uuid, is_favorited: bool) -> Result<Workflow> {
        let id_s = id.to_string();
        let now_s = Utc::now().to_rfc3339();
        let fav_i = is_favorited as i64;

        let res: Workflow = self
            .db
            .with_conn(move |c| {
                let n = c
                    .execute(
                        "UPDATE workflows SET is_favorited = ?1, updated_at = ?2 \
                         WHERE id = ?3 AND deleted = 0",
                        params![fav_i, now_s, id_s],
                    )
                    .map_err(|e| AcsError::Storage(format!("UPDATE is_favorited failed: {}", e)))?;
                if n == 0 {
                    return Err(AcsError::NotFound(format!(
                        "Workflow with id '{}' not found",
                        id
                    )));
                }
                let mut stmt = c
                    .prepare("SELECT * FROM workflows WHERE id = ?")
                    .map_err(|e| AcsError::Storage(e.to_string()))?;
                stmt.query_row([&id.to_string()], row_to_workflow)
                    .optional()
                    .map_err(|e| AcsError::Storage(e.to_string()))?
                    .ok_or_else(|| {
                        AcsError::NotFound(format!("Workflow with id '{}' not found", id))
                    })
            })
            .await?;
        Ok(res)
    }

    async fn record_run_outcome(
        &self,
        workflow_id: Uuid,
        run_id: Uuid,
        status: RunStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<()> {
        let status_s = run_status_str(&status)?;
        let id_s = workflow_id.to_string();
        let run_id_s = run_id.to_string();
        let finished_s = finished_at.to_rfc3339();
        // updated_at is bumped to now() so consumers (UI, /health) can tell
        // the row changed; version is intentionally NOT bumped because this
        // is runtime metadata, not a definition change.
        let now_s = Utc::now().to_rfc3339();

        self.db
            .with_conn(move |c| {
                let n = c
                    .execute(
                        "UPDATE workflows SET
                            last_run_id = ?1,
                            last_run_status = ?2,
                            last_run_at = ?3,
                            updated_at = ?4
                         WHERE id = ?5",
                        params![run_id_s, status_s, finished_s, now_s, id_s],
                    )
                    .map_err(|e| {
                        AcsError::Storage(format!("UPDATE workflow run-outcome failed: {}", e))
                    })?;
                if n == 0 {
                    return Err(AcsError::NotFound(format!(
                        "Workflow with id '{}' not found",
                        workflow_id
                    )));
                }
                Ok(())
            })
            .await?;
        Ok(())
    }
}

// ─── INSERT / UPDATE helpers ─────────────────────────────────────────────────

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

    let last_run_status = wf
        .last_run_status
        .as_ref()
        .map(run_status_str)
        .transpose()?;

    conn.execute(
        "INSERT INTO workflows (
            id, name, version, schedule, timezone, schedule_mode, enabled,
            steps_json, default_input, working_dir, env_vars,
            allow_concurrent, on_failure, last_run_at, last_run_status,
            last_run_id, created_at, updated_at, is_favorited
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7,
            ?8, ?9, ?10, ?11,
            ?12, ?13, ?14, ?15,
            ?16, ?17, ?18, ?19
        )",
        params![
            wf.id.to_string(),
            wf.name,
            wf.version as i64,
            wf.schedule,
            wf.timezone,
            schedule_mode_str(&wf.schedule_mode)?,
            wf.enabled as i64,
            steps_json,
            default_input,
            wf.working_dir,
            env_vars,
            wf.allow_concurrent as i64,
            on_failure_blob(&wf.on_failure)?,
            wf.last_run_at.map(|d| d.to_rfc3339()),
            last_run_status,
            wf.last_run_id.map(|u| u.to_string()),
            wf.created_at.to_rfc3339(),
            wf.updated_at.to_rfc3339(),
            wf.is_favorited as i64,
        ],
    )
    .map_err(|e| map_insert_err(&wf.name, e))?;
    Ok(())
}

fn update_workflow_row(conn: &Connection, wf: &Workflow) -> Result<(), AcsError> {
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
    let last_run_status = wf
        .last_run_status
        .as_ref()
        .map(run_status_str)
        .transpose()?;

    conn.execute(
        "UPDATE workflows SET
            name = ?1, version = ?2, schedule = ?3, timezone = ?4,
            schedule_mode = ?5, enabled = ?6, steps_json = ?7,
            default_input = ?8, working_dir = ?9,
            env_vars = ?10, allow_concurrent = ?11, on_failure = ?12,
            last_run_at = ?13, last_run_status = ?14, last_run_id = ?15,
            updated_at = ?16, is_favorited = ?17
         WHERE id = ?18",
        params![
            wf.name,
            wf.version as i64,
            wf.schedule,
            wf.timezone,
            schedule_mode_str(&wf.schedule_mode)?,
            wf.enabled as i64,
            steps_json,
            default_input,
            wf.working_dir,
            env_vars,
            wf.allow_concurrent as i64,
            on_failure_blob(&wf.on_failure)?,
            wf.last_run_at.map(|d| d.to_rfc3339()),
            last_run_status,
            wf.last_run_id.map(|u| u.to_string()),
            wf.updated_at.to_rfc3339(),
            wf.is_favorited as i64,
            wf.id.to_string(),
        ],
    )
    .map_err(|e| AcsError::Storage(format!("UPDATE workflow failed: {}", e)))?;
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, ScheduleMode, ShellStep, StepDef, StepDefCommon,
    };
    use std::collections::HashMap;

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

    fn minimal_new_workflow(name: &str) -> NewWorkflow {
        NewWorkflow {
            name: name.to_string(),
            schedule: "*/5 * * * *".to_string(),
            timezone: None,
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: None,
            on_failure: FailurePolicy::default(),
        }
    }

    fn populated_new_workflow(name: &str) -> NewWorkflow {
        let mut env = HashMap::new();
        env.insert("FOO".to_string(), "bar".to_string());
        NewWorkflow {
            name: name.to_string(),
            schedule: "0 * * * *".to_string(),
            timezone: Some("America/New_York".to_string()),
            schedule_mode: ScheduleMode::WaitForCompletion,
            enabled: false,
            steps: vec![make_shell_step("a"), make_shell_step("b")],
            default_input: Some(serde_json::json!({"k": 1})),
            working_dir: Some("/tmp".to_string()),
            env_vars: Some(env),
            allow_concurrent: Some(false),
            on_failure: FailurePolicy::Retry {
                attempts: 3,
                backoff_ms: 1000,
            },
        }
    }

    // ── Round-trips ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_then_get_minimal_round_trip() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("min"))
            .await
            .expect("create");
        let got = store
            .get_workflow(created.id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(created, got);
    }

    #[tokio::test]
    async fn test_create_then_get_populated_round_trip() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(populated_new_workflow("populated"))
            .await
            .expect("create");
        let got = store
            .get_workflow(created.id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(created, got);
        // Spot-check the fields that were optional & populated.
        assert_eq!(got.timezone.as_deref(), Some("America/New_York"));
        assert_eq!(got.schedule_mode, ScheduleMode::WaitForCompletion);
        assert!(!got.allow_concurrent);
        assert!(matches!(
            got.on_failure,
            FailurePolicy::Retry {
                attempts: 3,
                backoff_ms: 1000
            }
        ));
    }

    #[tokio::test]
    async fn test_get_workflow_not_found_returns_none() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let result = store.get_workflow(Uuid::now_v7()).await.expect("get");
        assert!(result.is_none());
    }

    // ── find_by_name ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_find_by_name_hit() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        store
            .create_workflow(minimal_new_workflow("findme"))
            .await
            .expect("create");
        let got = store
            .find_by_name("findme")
            .await
            .expect("find")
            .expect("present");
        assert_eq!(got.name, "findme");
    }

    #[tokio::test]
    async fn test_find_by_name_miss() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let got = store.find_by_name("nope").await.expect("find");
        assert!(got.is_none());
    }

    // ── Duplicate name ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_duplicate_name_returns_conflict() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        store
            .create_workflow(minimal_new_workflow("dup"))
            .await
            .expect("first");
        let err = store
            .create_workflow(minimal_new_workflow("dup"))
            .await
            .expect_err("second must fail");
        // anyhow::Error wrapping AcsError::Conflict
        let msg = err.to_string();
        assert!(
            msg.contains("already exists") || msg.contains("Conflict"),
            "got: {}",
            msg
        );
        // The underlying AcsError variant must be Conflict.
        let acs = err.downcast_ref::<AcsError>().expect("must be AcsError");
        assert!(matches!(acs, AcsError::Conflict(_)));
    }

    #[tokio::test]
    async fn test_update_to_existing_name_returns_conflict() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        store
            .create_workflow(minimal_new_workflow("a"))
            .await
            .expect("a");
        let b = store
            .create_workflow(minimal_new_workflow("b"))
            .await
            .expect("b");
        let err = store
            .update_workflow(
                b.id,
                WorkflowUpdate {
                    name: Some("a".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("rename to a must fail");
        let acs = err.downcast_ref::<AcsError>().expect("must be AcsError");
        assert!(matches!(acs, AcsError::Conflict(_)));
    }

    // ── Update semantics ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_only_changes_supplied_fields() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(populated_new_workflow("upd"))
            .await
            .expect("create");
        let updated = store
            .update_workflow(
                created.id,
                WorkflowUpdate {
                    schedule: Some("0 0 * * *".to_string()),
                    ..Default::default()
                },
            )
            .await
            .expect("update");
        assert_eq!(updated.schedule, "0 0 * * *");
        // Other fields preserved.
        assert_eq!(updated.timezone, created.timezone);
        assert_eq!(updated.steps, created.steps);
        assert_eq!(updated.env_vars, created.env_vars);
        assert_eq!(updated.allow_concurrent, created.allow_concurrent);
        assert_eq!(updated.on_failure, created.on_failure);
    }

    #[tokio::test]
    async fn test_update_steps_bumps_version() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("v"))
            .await
            .expect("create");
        assert_eq!(created.version, 1);
        let updated = store
            .update_workflow(
                created.id,
                WorkflowUpdate {
                    steps: Some(vec![make_shell_step("s1"), make_shell_step("s2")]),
                    ..Default::default()
                },
            )
            .await
            .expect("update");
        assert_eq!(updated.version, 2);
    }

    // ── set_favorited ─────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_set_favorited_round_trip() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("fav-1"))
            .await
            .expect("create");
        assert!(!created.is_favorited, "fresh workflows default to false");
        let initial_version = created.version;

        let updated = store
            .set_favorited(created.id, true)
            .await
            .expect("favorite");
        assert!(updated.is_favorited);
        // is_favorited must NOT bump version.
        assert_eq!(updated.version, initial_version);

        let got = store
            .get_workflow(created.id)
            .await
            .expect("get")
            .expect("present");
        assert!(got.is_favorited);

        // Toggling back to false also returns the updated record.
        let unfaved = store
            .set_favorited(created.id, false)
            .await
            .expect("unfavorite");
        assert!(!unfaved.is_favorited);
    }

    #[tokio::test]
    async fn test_set_favorited_unknown_workflow_returns_not_found() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let err = store
            .set_favorited(Uuid::now_v7(), true)
            .await
            .expect_err("must error");
        let acs = err.downcast_ref::<AcsError>().expect("must be AcsError");
        assert!(matches!(acs, AcsError::NotFound(_)));
    }

    #[tokio::test]
    async fn test_update_is_favorited_via_patch_does_not_bump_version() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("fav-patch"))
            .await
            .expect("create");
        assert_eq!(created.version, 1);
        let updated = store
            .update_workflow(
                created.id,
                WorkflowUpdate {
                    is_favorited: Some(true),
                    ..Default::default()
                },
            )
            .await
            .expect("update");
        assert!(updated.is_favorited);
        assert_eq!(updated.version, 1);
    }

    #[tokio::test]
    async fn test_update_enabled_only_does_not_bump_version() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("nv"))
            .await
            .expect("create");
        assert_eq!(created.version, 1);
        let updated = store
            .update_workflow(
                created.id,
                WorkflowUpdate {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
            .await
            .expect("update");
        assert_eq!(updated.version, 1);
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn test_update_workflow_not_found() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let err = store
            .update_workflow(
                Uuid::now_v7(),
                WorkflowUpdate {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
            .await
            .expect_err("must error");
        let acs = err.downcast_ref::<AcsError>().expect("must be AcsError");
        assert!(matches!(acs, AcsError::NotFound(_)));
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_then_get_returns_none() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("del"))
            .await
            .expect("create");
        store.delete_workflow(created.id).await.expect("delete");
        let got = store.get_workflow(created.id).await.expect("get");
        assert!(got.is_none());
    }

    #[tokio::test]
    async fn test_delete_workflow_not_found() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let err = store
            .delete_workflow(Uuid::now_v7())
            .await
            .expect_err("must error");
        let acs = err.downcast_ref::<AcsError>().expect("must be AcsError");
        assert!(matches!(acs, AcsError::NotFound(_)));
    }

    // ── Soft delete (ACS-25) ────────────────────────────────────────────────

    /// A soft-deleted workflow is excluded from `list_workflows`,
    /// `find_by_name`, and `get_workflow` — i.e. it is hidden from the UI and,
    /// because the scheduler enumerates via `list_workflows`, from scheduling.
    #[tokio::test]
    async fn test_soft_delete_excluded_from_list_get_find() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("sd-hidden"))
            .await
            .expect("create");
        store.delete_workflow(created.id).await.expect("delete");

        assert!(
            store.list_workflows().await.expect("list").is_empty(),
            "soft-deleted workflow must be absent from list_workflows (and thus the scheduler)"
        );
        assert!(
            store.get_workflow(created.id).await.expect("get").is_none(),
            "get_workflow must treat a soft-deleted workflow as not found"
        );
        assert!(
            store
                .find_by_name("sd-hidden")
                .await
                .expect("find")
                .is_none(),
            "find_by_name must not resolve a soft-deleted workflow"
        );
    }

    /// The `*_including_deleted` accessors still surface the row, flagged
    /// `deleted = true`, so the cost endpoints can report its history.
    #[tokio::test]
    async fn test_soft_delete_visible_via_including_deleted() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("sd-cost"))
            .await
            .expect("create");
        assert!(!created.deleted, "fresh workflow is not deleted");
        store.delete_workflow(created.id).await.expect("delete");

        let all = store
            .list_workflows_including_deleted()
            .await
            .expect("list incl deleted");
        assert_eq!(all.len(), 1);
        assert!(all[0].deleted, "row must be flagged deleted");
        assert_eq!(
            all[0].name, "sd-cost",
            "deleted rows keep their name verbatim"
        );

        let got = store
            .get_workflow_including_deleted(created.id)
            .await
            .expect("get incl deleted")
            .expect("present");
        assert!(got.deleted);
        assert_eq!(got.name, "sd-cost");
    }

    /// Deleting an already-deleted workflow returns NotFound (idempotency at
    /// the API boundary — the row is gone from the "live" set).
    #[tokio::test]
    async fn test_soft_delete_twice_returns_not_found() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("sd-twice"))
            .await
            .expect("create");
        store
            .delete_workflow(created.id)
            .await
            .expect("first delete");
        let err = store
            .delete_workflow(created.id)
            .await
            .expect_err("second delete must fail");
        let acs = err.downcast_ref::<AcsError>().expect("AcsError");
        assert!(matches!(acs, AcsError::NotFound(_)));
    }

    /// A name can be reused after its owner is soft-deleted; the two workflows
    /// remain distinct rows (distinct ids), so cost views key by id stay
    /// separate.
    #[tokio::test]
    async fn test_soft_delete_frees_name_for_reuse() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let first = store
            .create_workflow(minimal_new_workflow("reuse-me"))
            .await
            .expect("create first");
        store.delete_workflow(first.id).await.expect("delete first");

        // Recreating with the same name must succeed (the UNIQUE slot is freed).
        let second = store
            .create_workflow(minimal_new_workflow("reuse-me"))
            .await
            .expect("recreate same name");
        assert_ne!(first.id, second.id, "recreated workflow has a new id");

        // find_by_name resolves the live one; list shows only the live one.
        let found = store
            .find_by_name("reuse-me")
            .await
            .expect("find")
            .expect("present");
        assert_eq!(found.id, second.id);
        let live = store.list_workflows().await.expect("list");
        assert_eq!(live.len(), 1);
        assert_eq!(live[0].id, second.id);

        // Both rows survive for cost purposes, keyed by distinct ids.
        let all = store
            .list_workflows_including_deleted()
            .await
            .expect("list incl deleted");
        assert_eq!(all.len(), 2, "deleted + live rows both persist");
    }

    /// Updating or favoriting a soft-deleted workflow is a NotFound.
    #[tokio::test]
    async fn test_update_and_favorite_on_deleted_return_not_found() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("sd-upd"))
            .await
            .expect("create");
        store.delete_workflow(created.id).await.expect("delete");

        let upd_err = store
            .update_workflow(
                created.id,
                WorkflowUpdate {
                    enabled: Some(false),
                    ..Default::default()
                },
            )
            .await
            .expect_err("update must fail");
        assert!(matches!(
            upd_err.downcast_ref::<AcsError>().expect("AcsError"),
            AcsError::NotFound(_)
        ));

        let fav_err = store
            .set_favorited(created.id, true)
            .await
            .expect_err("favorite must fail");
        assert!(matches!(
            fav_err.downcast_ref::<AcsError>().expect("AcsError"),
            AcsError::NotFound(_)
        ));
    }

    // ── List ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_workflows_empty() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let v = store.list_workflows().await.expect("list");
        assert!(v.is_empty());
    }

    #[tokio::test]
    async fn test_list_workflows_returns_all() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        for n in ["a", "b", "c"] {
            store
                .create_workflow(minimal_new_workflow(n))
                .await
                .expect("create");
        }
        let mut names: Vec<_> = store
            .list_workflows()
            .await
            .expect("list")
            .into_iter()
            .map(|w| w.name)
            .collect();
        names.sort();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    // ── record_run_outcome ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_record_run_outcome_populates_last_run_fields() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("rro-1"))
            .await
            .expect("create");
        // Pre-condition: last_run_* are null on a freshly created workflow.
        assert!(created.last_run_id.is_none());
        assert!(created.last_run_status.is_none());
        assert!(created.last_run_at.is_none());
        let initial_version = created.version;

        let run_id = Uuid::now_v7();
        let finished_at = Utc::now();
        store
            .record_run_outcome(created.id, run_id, RunStatus::Completed, finished_at)
            .await
            .expect("record_run_outcome");

        let got = store
            .get_workflow(created.id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(got.last_run_id, Some(run_id));
        assert_eq!(got.last_run_status, Some(RunStatus::Completed));
        // Round-tripping through RFC3339 → SQLite TEXT → RFC3339 truncates
        // sub-second precision below microseconds depending on chrono. Compare
        // by RFC3339 string at second precision to be robust.
        assert_eq!(
            got.last_run_at.expect("set").timestamp(),
            finished_at.timestamp()
        );
        // version must NOT bump for runtime metadata changes.
        assert_eq!(got.version, initial_version);
    }

    #[tokio::test]
    async fn test_record_run_outcome_unknown_workflow_returns_not_found() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let err = store
            .record_run_outcome(
                Uuid::now_v7(),
                Uuid::now_v7(),
                RunStatus::Failed,
                Utc::now(),
            )
            .await
            .expect_err("must error");
        let acs = err.downcast_ref::<AcsError>().expect("must be AcsError");
        assert!(matches!(acs, AcsError::NotFound(_)), "got: {:?}", acs);
    }

    #[tokio::test]
    async fn test_record_run_outcome_latest_wins() {
        let store = SqliteWorkflowStore::in_memory_for_tests();
        let created = store
            .create_workflow(minimal_new_workflow("rro-3"))
            .await
            .expect("create");

        let first_run = Uuid::now_v7();
        let first_at = Utc::now();
        store
            .record_run_outcome(created.id, first_run, RunStatus::Failed, first_at)
            .await
            .expect("first record");

        // Sleep a tick so the finished_at timestamps are distinguishable
        // even on platforms whose clock has limited monotonicity.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        let second_run = Uuid::now_v7();
        let second_at = Utc::now();
        store
            .record_run_outcome(created.id, second_run, RunStatus::Completed, second_at)
            .await
            .expect("second record");

        let got = store
            .get_workflow(created.id)
            .await
            .expect("get")
            .expect("present");
        assert_eq!(
            got.last_run_id,
            Some(second_run),
            "second call's run_id wins"
        );
        assert_eq!(got.last_run_status, Some(RunStatus::Completed));
        assert_eq!(
            got.last_run_at.expect("set").timestamp(),
            second_at.timestamp()
        );
    }
}
