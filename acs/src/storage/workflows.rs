use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::{validate_new_workflow, validate_workflow_update};
use crate::models::workflow::{NewWorkflow, Workflow, WorkflowUpdate};

// ─── WorkflowStore trait ──────────────────────────────────────────────────────

#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn list_workflows(&self) -> Result<Vec<Workflow>>;
    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>>;
    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow>;
    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow>;
    async fn delete_workflow(&self, id: Uuid) -> Result<()>;
}

// ─── FsWorkflowStore ─────────────────────────────────────────────────────────

pub struct FsWorkflowStore {
    path: PathBuf,
    inner: RwLock<Vec<Workflow>>,
}

impl FsWorkflowStore {
    /// Create a new FsWorkflowStore, loading existing data from disk if present.
    ///
    /// If `workflows.json` is corrupted (invalid JSON), creates a timestamped
    /// backup (e.g. `workflows.json.bak.<timestamp>`), logs a warning, and
    /// starts with an empty workflow list.
    pub async fn new(data_dir: &Path) -> Result<Self> {
        tokio::fs::create_dir_all(data_dir)
            .await
            .context("Failed to create data directory")?;

        let path = data_dir.join("workflows.json");

        let workflows = if path.exists() {
            let content = tokio::fs::read_to_string(&path)
                .await
                .context("Failed to read workflows.json")?;
            match serde_json::from_str::<Vec<Workflow>>(&content) {
                Ok(parsed) => parsed,
                Err(e) => {
                    // Corrupted JSON: create timestamped backup and start fresh
                    tracing::warn!(
                        "workflows.json is corrupted ({}), creating backup and starting empty",
                        e
                    );
                    let ts = Utc::now().timestamp();
                    let backup_path = data_dir.join(format!("workflows.json.bak.{}", ts));
                    if let Err(backup_err) = tokio::fs::copy(&path, &backup_path).await {
                        tracing::error!(
                            "Failed to create backup of corrupted workflows.json: {}",
                            backup_err
                        );
                    }
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(Self {
            path,
            inner: RwLock::new(workflows),
        })
    }

    /// Atomically write the workflows cache to disk.
    /// Writes to a .tmp file first, then renames to the actual file.
    async fn persist(&self, workflows: &[Workflow]) -> Result<()> {
        let tmp_path = self.path.with_extension("json.tmp");

        let json =
            serde_json::to_string_pretty(workflows).context("Failed to serialize workflows")?;

        tokio::fs::write(&tmp_path, json.as_bytes())
            .await
            .context("Failed to write temporary workflows file")?;

        tokio::fs::rename(&tmp_path, &self.path)
            .await
            .context("Failed to rename temporary workflows file")?;

        Ok(())
    }
}

#[async_trait]
impl WorkflowStore for FsWorkflowStore {
    async fn list_workflows(&self) -> Result<Vec<Workflow>> {
        let inner = self.inner.read().await;
        Ok(inner.clone())
    }

    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>> {
        let inner = self.inner.read().await;
        Ok(inner.iter().find(|w| w.id == id).cloned())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>> {
        let inner = self.inner.read().await;
        Ok(inner.iter().find(|w| w.name == name).cloned())
    }

    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow> {
        validate_new_workflow(&new)?;

        let mut inner = self.inner.write().await;

        // Check for duplicate name
        if inner.iter().any(|w| w.name == new.name) {
            return Err(AcsError::Conflict(format!(
                "A workflow with name '{}' already exists",
                new.name
            ))
            .into());
        }

        let now = Utc::now();
        let workflow = Workflow {
            id: Uuid::now_v7(),
            name: new.name,
            version: 1,
            schedule: new.schedule,
            timezone: new.timezone,
            schedule_mode: new.schedule_mode,
            enabled: new.enabled,
            steps: new.steps,
            input_schema: new.input_schema,
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

        inner.push(workflow.clone());
        self.persist(&inner).await?;

        Ok(workflow)
    }

    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow> {
        validate_workflow_update(&update)?;

        let mut inner = self.inner.write().await;

        let idx = inner
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| AcsError::NotFound(format!("Workflow with id '{}' not found", id)))?;

        // Check for duplicate name (excluding self)
        if let Some(ref new_name) = update.name {
            if inner.iter().any(|w| w.name == *new_name && w.id != id) {
                return Err(AcsError::Conflict(format!(
                    "A workflow with name '{}' already exists",
                    new_name
                ))
                .into());
            }
        }

        let wf = &mut inner[idx];

        // Track whether any definition-affecting field changed.
        //
        // Definition-affecting fields are those that change the workflow's
        // behaviour or structure: steps, on_failure, input_schema,
        // default_input, working_dir, env_vars, allow_concurrent, schedule,
        // schedule_mode, timezone, and name.
        //
        // NOT definition-affecting (runtime metadata set by the daemon):
        //   last_run_at, last_run_status, last_run_id  — these are not
        //   present in WorkflowUpdate at all.
        //
        // The `enabled` flag is intentionally excluded: toggling enabled/
        // disabled does not alter the workflow definition, so it does NOT
        // bump the version.
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
            // enabled does NOT bump version
            wf.enabled = enabled;
        }
        if let Some(steps) = update.steps {
            if wf.steps != steps {
                definition_changed = true;
            }
            wf.steps = steps;
        }
        if let Some(input_schema) = update.input_schema {
            if wf.input_schema.as_ref() != Some(&input_schema) {
                definition_changed = true;
            }
            wf.input_schema = Some(input_schema);
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

        let updated = wf.clone();
        self.persist(&inner).await?;

        Ok(updated)
    }

    async fn delete_workflow(&self, id: Uuid) -> Result<()> {
        let mut inner = self.inner.write().await;

        let idx = inner
            .iter()
            .position(|w| w.id == id)
            .ok_or_else(|| AcsError::NotFound(format!("Workflow with id '{}' not found", id)))?;

        inner.remove(idx);
        self.persist(&inner).await?;

        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::models::workflow::{
        AgentStep, AgentType, CaptureSpec, FailurePolicy, HttpStep, MatchStep, ScriptStep,
        SetVarStep, ShellStep, StepDef, StepDefCommon,
    };
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

    fn make_test_workflow(name: &str) -> NewWorkflow {
        NewWorkflow {
            name: name.to_string(),
            schedule: "*/5 * * * *".to_string(),
            timezone: None,
            schedule_mode: Default::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            input_schema: None,
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: None,
            on_failure: FailurePolicy::default(),
        }
    }

    async fn setup_store() -> (FsWorkflowStore, TempDir) {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let store = FsWorkflowStore::new(tmp_dir.path())
            .await
            .expect("create store");
        (store, tmp_dir)
    }

    // ── Atomic write ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_atomic_write_produces_valid_json() {
        let (store, tmp) = setup_store().await;
        store
            .create_workflow(make_test_workflow("persist-test"))
            .await
            .expect("create");

        let file_path = tmp.path().join("workflows.json");
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .expect("read file");
        let workflows: Vec<Workflow> = serde_json::from_str(&content).expect("parse JSON");
        assert_eq!(workflows.len(), 1);
        assert_eq!(workflows[0].name, "persist-test");
    }

    // ── Corruption recovery ───────────────────────────────────────────────────

    #[tokio::test]
    async fn test_corrupted_workflows_json_creates_backup() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let wf_file = tmp_dir.path().join("workflows.json");

        let corrupted_content = b"corrupted data!!!";
        tokio::fs::write(&wf_file, corrupted_content)
            .await
            .expect("write corrupted file");

        let _store = FsWorkflowStore::new(tmp_dir.path())
            .await
            .expect("create store");

        // Verify a backup file was created (timestamped)
        let entries = std::fs::read_dir(tmp_dir.path()).expect("read dir");
        let backup_exists = entries
            .filter_map(|e| e.ok())
            .any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("workflows.json.bak.")
            });
        assert!(backup_exists, "Timestamped backup file should have been created");
    }

    #[tokio::test]
    async fn test_corrupted_workflows_json_recovers_empty() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let wf_file = tmp_dir.path().join("workflows.json");

        tokio::fs::write(&wf_file, b"this is not valid JSON{{{")
            .await
            .expect("write corrupted file");

        let store = FsWorkflowStore::new(tmp_dir.path())
            .await
            .expect("create store from corrupted file");

        let workflows = store.list_workflows().await.expect("list");
        assert!(
            workflows.is_empty(),
            "Should start with empty workflows after corruption"
        );
    }

    #[tokio::test]
    async fn test_corrupted_workflows_json_can_still_create_workflows() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let wf_file = tmp_dir.path().join("workflows.json");

        tokio::fs::write(&wf_file, b"not json")
            .await
            .expect("write corrupted file");

        let store = FsWorkflowStore::new(tmp_dir.path())
            .await
            .expect("create store");

        let wf = store
            .create_workflow(make_test_workflow("new-after-corrupt"))
            .await
            .expect("create");
        assert_eq!(wf.name, "new-after-corrupt");

        let workflows = store.list_workflows().await.expect("list");
        assert_eq!(workflows.len(), 1);
    }

    // ── Create ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_create_workflow() {
        let (store, _tmp) = setup_store().await;
        let wf = store
            .create_workflow(make_test_workflow("my-workflow"))
            .await
            .expect("create");
        assert_eq!(wf.name, "my-workflow");
        assert_eq!(wf.schedule, "*/5 * * * *");
        assert!(wf.enabled);
        assert_eq!(wf.version, 1);
        assert!(wf.last_run_at.is_none());
        assert!(wf.last_run_status.is_none());
        assert!(wf.last_run_id.is_none());
        assert!(wf.next_run_at.is_none());
    }

    #[tokio::test]
    async fn test_create_workflow_with_steps() {
        let (store, _tmp) = setup_store().await;

        // Build a workflow that exercises all 6 step kinds
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());

        let mut cases = HashMap::new();
        cases.insert("ok".to_string(), vec![make_shell_step("match-ok")]);

        let mut exports = HashMap::new();
        exports.insert("MY_VAR".to_string(), "value".to_string());

        let steps = vec![
            make_shell_step("step-shell"),
            StepDef::Script(ScriptStep {
                common: StepDefCommon {
                    id: "step-script".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                path: "/usr/local/bin/deploy.sh".to_string(),
                script_type: Some("bash".to_string()),
                args: None,
                pass_stdin: false,
            }),
            StepDef::Http(HttpStep {
                common: StepDefCommon {
                    id: "step-http".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: Some(10),
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                method: "POST".to_string(),
                url: "https://example.com/api".to_string(),
                headers,
                body: None,
                expect_status: vec![200],
            }),
            StepDef::Match(MatchStep {
                common: StepDefCommon {
                    id: "step-match".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                expr: "${exit_code}".to_string(),
                cases,
                default: None,
            }),
            StepDef::SetVar(SetVarStep {
                common: StepDefCommon {
                    id: "step-setvar".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                exports,
            }),
            StepDef::Agent(AgentStep {
                common: StepDefCommon {
                    id: "step-agent".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: Some(120),
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                agent_type: AgentType::ClaudeCodeCli,
                prompt: "Analyze the output".to_string(),
                command_template: None,
            }),
        ];

        let new_wf = NewWorkflow {
            name: "all-step-kinds".to_string(),
            schedule: "0 * * * *".to_string(),
            steps,
            ..make_test_workflow("all-step-kinds")
        };

        let wf = store.create_workflow(new_wf).await.expect("create");
        assert_eq!(wf.steps.len(), 6);
        assert_eq!(wf.version, 1);
    }

    // ── Delete ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_delete_workflow() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_workflow(make_test_workflow("to-delete"))
            .await
            .expect("create");
        store.delete_workflow(created.id).await.expect("delete");
        let result = store.get_workflow(created.id).await.expect("get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_workflow_not_found() {
        let (store, _tmp) = setup_store().await;
        let result = store.delete_workflow(Uuid::now_v7()).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("Not found"),
            "Expected NotFound error, got: {}",
            err
        );
    }

    // ── Duplicate name ────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_duplicate_name_returns_error() {
        let (store, _tmp) = setup_store().await;
        store
            .create_workflow(make_test_workflow("dup-name"))
            .await
            .expect("create first");
        let result = store.create_workflow(make_test_workflow("dup-name")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("already exists") || err_str.contains("Conflict"),
            "Expected conflict error, got: {}",
            err_str
        );
    }

    // ── Find by name ──────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_find_by_name() {
        let (store, _tmp) = setup_store().await;
        store
            .create_workflow(make_test_workflow("find-me"))
            .await
            .expect("create");
        store
            .create_workflow(make_test_workflow("not-me"))
            .await
            .expect("create");
        let found = store
            .find_by_name("find-me")
            .await
            .expect("find")
            .expect("found");
        assert_eq!(found.name, "find-me");
    }

    #[tokio::test]
    async fn test_find_by_name_not_found() {
        let (store, _tmp) = setup_store().await;
        let result = store.find_by_name("nonexistent").await.expect("find");
        assert!(result.is_none());
    }

    // ── Get ───────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_get_workflow() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_workflow(make_test_workflow("get-me"))
            .await
            .expect("create");
        let fetched = store
            .get_workflow(created.id)
            .await
            .expect("get")
            .expect("found");
        assert_eq!(created.id, fetched.id);
        assert_eq!(created.name, fetched.name);
    }

    #[tokio::test]
    async fn test_get_workflow_not_found() {
        let (store, _tmp) = setup_store().await;
        let result = store.get_workflow(Uuid::now_v7()).await.expect("get");
        assert!(result.is_none());
    }

    // ── List ──────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_list_workflows() {
        let (store, _tmp) = setup_store().await;
        store
            .create_workflow(make_test_workflow("wf-1"))
            .await
            .expect("create");
        store
            .create_workflow(make_test_workflow("wf-2"))
            .await
            .expect("create");
        store
            .create_workflow(make_test_workflow("wf-3"))
            .await
            .expect("create");
        let workflows = store.list_workflows().await.expect("list");
        assert_eq!(workflows.len(), 3);
    }

    #[tokio::test]
    async fn test_list_workflows_empty() {
        let (store, _tmp) = setup_store().await;
        let workflows = store.list_workflows().await.expect("list");
        assert!(workflows.is_empty());
    }

    // ── No tmp file left ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_no_tmp_file_left_after_write() {
        let (store, tmp) = setup_store().await;
        store
            .create_workflow(make_test_workflow("clean-write"))
            .await
            .expect("create");

        let tmp_file = tmp.path().join("workflows.json.tmp");
        assert!(
            !tmp_file.exists(),
            "Temporary file should not remain after write"
        );
    }

    // ── Persistence across instances ──────────────────────────────────────────

    #[tokio::test]
    async fn test_persistence_across_instances() {
        let tmp_dir = TempDir::new().expect("create temp dir");

        {
            let store = FsWorkflowStore::new(tmp_dir.path())
                .await
                .expect("create store");
            store
                .create_workflow(make_test_workflow("persistent-wf"))
                .await
                .expect("create");
        }

        {
            let store = FsWorkflowStore::new(tmp_dir.path())
                .await
                .expect("create store");
            let workflows = store.list_workflows().await.expect("list");
            assert_eq!(workflows.len(), 1);
            assert_eq!(workflows[0].name, "persistent-wf");
        }
    }

    // ── Update ────────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_update_workflow_not_found() {
        let (store, _tmp) = setup_store().await;
        let update = WorkflowUpdate {
            name: Some("new-name".to_string()),
            ..Default::default()
        };
        let result = store.update_workflow(Uuid::now_v7(), update).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not found") || err.to_string().contains("Not found"),
            "Expected NotFound error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_update_workflow() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_workflow(make_test_workflow("original-name"))
            .await
            .expect("create");

        let update = WorkflowUpdate {
            name: Some("updated-name".to_string()),
            schedule: Some("0 * * * *".to_string()),
            enabled: Some(false),
            ..Default::default()
        };

        let updated = store
            .update_workflow(created.id, update)
            .await
            .expect("update");
        assert_eq!(updated.name, "updated-name");
        assert_eq!(updated.schedule, "0 * * * *");
        assert!(!updated.enabled);
        assert!(updated.updated_at >= created.updated_at);
        // name and schedule changed — version should bump
        assert_eq!(updated.version, 2);
    }

    #[tokio::test]
    async fn test_update_workflow_bumps_version_on_steps_change() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_workflow(make_test_workflow("version-test"))
            .await
            .expect("create");
        assert_eq!(created.version, 1);

        let new_steps = vec![
            make_shell_step("step-1"),
            make_shell_step("step-2"),
        ];
        let update = WorkflowUpdate {
            steps: Some(new_steps),
            ..Default::default()
        };

        let updated = store
            .update_workflow(created.id, update)
            .await
            .expect("update");
        assert_eq!(updated.version, 2);
    }

    #[tokio::test]
    async fn test_update_workflow_does_not_bump_version_on_enabled_only() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_workflow(make_test_workflow("no-bump-test"))
            .await
            .expect("create");
        assert_eq!(created.version, 1);

        let update = WorkflowUpdate {
            enabled: Some(false),
            ..Default::default()
        };

        let updated = store
            .update_workflow(created.id, update)
            .await
            .expect("update");
        // enabled-only change must NOT bump version
        assert_eq!(updated.version, 1);
        assert!(!updated.enabled);
    }

    #[tokio::test]
    async fn test_update_duplicate_name_returns_error() {
        let (store, _tmp) = setup_store().await;
        store
            .create_workflow(make_test_workflow("wf-a"))
            .await
            .expect("create");
        let wf_b = store
            .create_workflow(make_test_workflow("wf-b"))
            .await
            .expect("create");
        let update = WorkflowUpdate {
            name: Some("wf-a".to_string()),
            ..Default::default()
        };
        let result = store.update_workflow(wf_b.id, update).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("already exists") || err.to_string().contains("Conflict"),
            "Expected Conflict error, got: {}",
            err
        );
    }

    #[tokio::test]
    async fn test_update_workflow_with_steps() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_workflow(make_test_workflow("steps-update"))
            .await
            .expect("create");

        let new_steps = vec![
            make_shell_step("new-step-1"),
            make_shell_step("new-step-2"),
            make_shell_step("new-step-3"),
        ];
        let update = WorkflowUpdate {
            steps: Some(new_steps),
            ..Default::default()
        };

        let updated = store
            .update_workflow(created.id, update)
            .await
            .expect("update");
        assert_eq!(updated.steps.len(), 3);
        assert_eq!(updated.version, 2);
    }
}
