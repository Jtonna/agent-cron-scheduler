//! Migration 001 — Convert legacy `jobs.json` to `workflows.json`.
//!
//! # Decision table
//!
//! 1. If `workflows.json` already exists → migration is already applied (no-op).
//! 2. If `jobs.json` does not exist → fresh install, nothing to migrate (no-op).
//! 3. Otherwise → reads `jobs.json`, synthesises one [`Workflow`] per [`Job`],
//!    writes `workflows.json`, and renames `jobs.json` to
//!    `jobs.json.migrated.<unix_timestamp>` as a backup.
//!
//! # Step synthesis rules
//!
//! For each Job, the synthesised Workflow has:
//! - A `pre_hook` step (if `job.pre_hook` is `Some`):
//!   - If `pre_hook_script_type` is `None` or `"shell"` → `ShellStep`
//!   - If `pre_hook_script_type` is a non-shell type (`"python"`, `"batch"`,
//!     `"powershell"`) → the hook body is written to a permanent script file at
//!     `<data_dir>/migrated_scripts/<job_id>_pre_hook.<ext>` and a `ScriptStep`
//!     pointing at that file is emitted.
//! - A main step derived from `job.execution`:
//!   - `ExecutionType::ShellCommand(cmd)` → `ShellStep`
//!   - `ExecutionType::ScriptFile(path)` → `ScriptStep` with `script_type`
//!     inferred from the file extension.
//! - A `post_hook` step (if `job.post_hook` is `Some`):
//!   - Same script_type logic as `pre_hook`, but with `always_run = true`.
//!     Non-shell types produce a `ScriptStep` at
//!     `<data_dir>/migrated_scripts/<job_id>_post_hook.<ext>`.
//!
//! Per-step `timeout_secs` is copied from `job.timeout_secs` when non-zero.

use std::path::Path;

use async_trait::async_trait;
use chrono::Utc;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::migration::legacy_types::{ExecutionType, Job};
use crate::migration::Migration;
use crate::models::workflow::{
    CaptureSpec, FailurePolicy, RunStatus, ScriptStep, ShellStep, StepDef, StepDefCommon, Workflow,
};

// ── Migration impl ────────────────────────────────────────────────────────────

/// Migration 001: convert `jobs.json` → `workflows.json`.
pub struct JobsToWorkflows;

#[async_trait]
impl Migration for JobsToWorkflows {
    fn name(&self) -> &'static str {
        "m001_jobs_to_workflows"
    }

    async fn run(&self, data_dir: &Path) -> Result<bool, AcsError> {
        let jobs_path = data_dir.join("jobs.json");
        let workflows_path = data_dir.join("workflows.json");

        // 1. workflows.json already exists → already applied.
        if workflows_path.exists() {
            return Ok(false);
        }

        // 2. jobs.json doesn't exist → fresh install, not needed.
        if !jobs_path.exists() {
            return Ok(false);
        }

        // 3. Read and parse jobs.json
        let jobs_json = tokio::fs::read_to_string(&jobs_path)
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to read jobs.json: {}", e)))?;

        let jobs: Vec<Job> = serde_json::from_str(&jobs_json)
            .map_err(|e| AcsError::Storage(format!("Failed to parse jobs.json: {}", e)))?;

        let count = jobs.len();

        // 4. Ensure migrated_scripts directory exists for non-shell hook bodies.
        let migrated_scripts_dir = data_dir.join("migrated_scripts");
        tokio::fs::create_dir_all(&migrated_scripts_dir)
            .await
            .map_err(|e| {
                AcsError::Storage(format!("Failed to create migrated_scripts dir: {}", e))
            })?;

        // 5. Synthesise workflows (may write script files into migrated_scripts_dir).
        let mut workflows: Vec<Workflow> = Vec::with_capacity(count);
        for job in jobs {
            let wf = synthesise_workflow_async(job, &migrated_scripts_dir).await?;
            workflows.push(wf);
        }

        // 6. Write workflows.json (atomic: write to .tmp, then rename)
        let tmp_path = data_dir.join("workflows.json.tmp");
        let json = serde_json::to_string_pretty(&workflows)
            .map_err(|e| AcsError::Storage(format!("Failed to serialize workflows: {}", e)))?;

        tokio::fs::write(&tmp_path, json.as_bytes())
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to write workflows.json.tmp: {}", e)))?;

        tokio::fs::rename(&tmp_path, &workflows_path)
            .await
            .map_err(|e| {
                AcsError::Storage(format!("Failed to rename workflows.json.tmp: {}", e))
            })?;

        // 7. Rename jobs.json to jobs.json.migrated.<timestamp> (backup; never delete)
        let ts = Utc::now().timestamp();
        let backup_path = data_dir.join(format!("jobs.json.migrated.{}", ts));
        tokio::fs::rename(&jobs_path, &backup_path)
            .await
            .map_err(|e| {
                AcsError::Storage(format!(
                    "Failed to rename jobs.json to backup ({}): {}",
                    backup_path.display(),
                    e
                ))
            })?;

        tracing::info!(
            "Migration 001 complete: {} job(s) → workflows.json. Original backed up to {}",
            count,
            backup_path.display()
        );

        Ok(true)
    }
}

// ── Synthesis ─────────────────────────────────────────────────────────────────

/// Return the file extension for a given `script_type` string.
fn ext_for_script_type(script_type: &str) -> &'static str {
    match script_type {
        "python" => "py",
        "batch" => "bat",
        "powershell" => "ps1",
        _ => "sh",
    }
}

/// Return `true` if the `script_type` value maps to a shell (inline) step.
fn is_shell_type(script_type: Option<&str>) -> bool {
    matches!(script_type, None | Some("shell"))
}

/// Synthesise the step for a pre- or post-hook.
///
/// Shell-type hooks (or unspecified) are emitted as `ShellStep`. Non-shell
/// types write the hook body to a permanent file under `migrated_scripts_dir`
/// and return a `ScriptStep` pointing at that file.
async fn synthesise_hook_step(
    hook_body: &str,
    script_type: Option<&str>,
    step_id: &str,
    always_run: bool,
    timeout: Option<u64>,
    working_dir: Option<String>,
    env_vars: Option<std::collections::HashMap<String, String>>,
    job_id: Uuid,
    migrated_scripts_dir: &Path,
) -> Result<StepDef, AcsError> {
    if is_shell_type(script_type) {
        return Ok(StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: step_id.to_string(),
                on_failure: Some(FailurePolicy::Abort),
                always_run,
                timeout_secs: timeout,
                working_dir,
                env_vars,
                capture: CaptureSpec::default(),
            },
            command: hook_body.to_string(),
            pass_stdin: false,
        }));
    }

    let st = script_type.unwrap(); // safe: is_shell_type returned false
    let ext = ext_for_script_type(st);
    let filename = format!("{}_{}.{}", job_id, step_id, ext);
    let script_path = migrated_scripts_dir.join(&filename);

    tokio::fs::write(&script_path, hook_body.as_bytes())
        .await
        .map_err(|e| {
            AcsError::Storage(format!(
                "Failed to write migrated hook script {}: {}",
                script_path.display(),
                e
            ))
        })?;

    Ok(StepDef::Script(ScriptStep {
        common: StepDefCommon {
            id: step_id.to_string(),
            on_failure: Some(FailurePolicy::Abort),
            always_run,
            timeout_secs: timeout,
            working_dir,
            env_vars,
            capture: CaptureSpec::default(),
        },
        path: script_path.to_string_lossy().into_owned(),
        script_type: Some(st.to_string()),
        args: None,
        pass_stdin: false,
    }))
}

/// Async synthesis used by the migration runner.
async fn synthesise_workflow_async(
    job: Job,
    migrated_scripts_dir: &Path,
) -> Result<Workflow, AcsError> {
    let timeout = if job.timeout_secs > 0 {
        Some(job.timeout_secs)
    } else {
        None
    };

    let mut steps: Vec<StepDef> = Vec::new();

    if let Some(ref hook_body) = job.pre_hook {
        let step = synthesise_hook_step(
            hook_body,
            job.pre_hook_script_type.as_deref(),
            "pre_hook",
            false,
            timeout,
            job.working_dir.clone(),
            job.env_vars.clone(),
            job.id,
            migrated_scripts_dir,
        )
        .await?;
        steps.push(step);
    }

    let main_step = match &job.execution {
        ExecutionType::ShellCommand(cmd) => StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: "main".to_string(),
                on_failure: Some(FailurePolicy::Abort),
                always_run: false,
                timeout_secs: timeout,
                working_dir: job.working_dir.clone(),
                env_vars: job.env_vars.clone(),
                capture: CaptureSpec::default(),
            },
            command: cmd.clone(),
            pass_stdin: false,
        }),
        ExecutionType::ScriptFile(path) => {
            let script_type = script_type_from_path(path);
            StepDef::Script(ScriptStep {
                common: StepDefCommon {
                    id: "main".to_string(),
                    on_failure: Some(FailurePolicy::Abort),
                    always_run: false,
                    timeout_secs: timeout,
                    working_dir: job.working_dir.clone(),
                    env_vars: job.env_vars.clone(),
                    capture: CaptureSpec::default(),
                },
                path: path.clone(),
                script_type,
                args: None,
                pass_stdin: false,
            })
        }
    };
    steps.push(main_step);

    if let Some(ref hook_body) = job.post_hook {
        let step = synthesise_hook_step(
            hook_body,
            job.post_hook_script_type.as_deref(),
            "post_hook",
            true,
            timeout,
            job.working_dir.clone(),
            job.env_vars.clone(),
            job.id,
            migrated_scripts_dir,
        )
        .await?;
        steps.push(step);
    }

    Ok(build_workflow(job, steps))
}

/// Synchronous hook-only synthesis used by unit tests (shell hooks only).
///
/// Panics if a hook has a non-shell `script_type`; use `synthesise_workflow_async`
/// for those cases.
#[cfg(test)]
fn synthesise_workflow(job: Job) -> Workflow {
    let timeout = if job.timeout_secs > 0 {
        Some(job.timeout_secs)
    } else {
        None
    };

    let mut steps: Vec<StepDef> = Vec::new();

    // ── pre_hook ──────────────────────────────────────────────────────────────
    if let Some(ref hook_cmd) = job.pre_hook {
        steps.push(StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: "pre_hook".to_string(),
                on_failure: Some(FailurePolicy::Abort),
                always_run: false,
                timeout_secs: timeout,
                working_dir: job.working_dir.clone(),
                env_vars: job.env_vars.clone(),
                capture: CaptureSpec::default(),
            },
            command: hook_cmd.clone(),
            pass_stdin: false,
        }));
    }

    // ── main step ─────────────────────────────────────────────────────────────
    let main_step = match &job.execution {
        ExecutionType::ShellCommand(cmd) => StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: "main".to_string(),
                on_failure: Some(FailurePolicy::Abort),
                always_run: false,
                timeout_secs: timeout,
                working_dir: job.working_dir.clone(),
                env_vars: job.env_vars.clone(),
                capture: CaptureSpec::default(),
            },
            command: cmd.clone(),
            pass_stdin: false,
        }),
        ExecutionType::ScriptFile(path) => {
            let script_type = script_type_from_path(path);
            StepDef::Script(ScriptStep {
                common: StepDefCommon {
                    id: "main".to_string(),
                    on_failure: Some(FailurePolicy::Abort),
                    always_run: false,
                    timeout_secs: timeout,
                    working_dir: job.working_dir.clone(),
                    env_vars: job.env_vars.clone(),
                    capture: CaptureSpec::default(),
                },
                path: path.clone(),
                script_type,
                args: None,
                pass_stdin: false,
            })
        }
    };
    steps.push(main_step);

    // ── post_hook ─────────────────────────────────────────────────────────────
    if let Some(ref hook_cmd) = job.post_hook {
        steps.push(StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: "post_hook".to_string(),
                on_failure: Some(FailurePolicy::Abort),
                always_run: true, // post hooks run even after failure
                timeout_secs: timeout,
                working_dir: job.working_dir.clone(),
                env_vars: job.env_vars.clone(),
                capture: CaptureSpec::default(),
            },
            command: hook_cmd.clone(),
            pass_stdin: false,
        }));
    }

    build_workflow(job, steps)
}

/// Assemble the final [`Workflow`] from a [`Job`] and its already-synthesised steps.
fn build_workflow(job: Job, steps: Vec<StepDef>) -> Workflow {
    let last_run_status = match job.last_exit_code {
        Some(0) => Some(RunStatus::Completed),
        Some(_) => Some(RunStatus::Failed),
        None => None,
    };

    Workflow {
        id: job.id,
        name: job.name,
        version: 1,
        schedule: job.schedule,
        timezone: job.timezone,
        schedule_mode: job.schedule_mode,
        enabled: job.enabled,
        is_favorited: false,
        steps,
        default_input: None,
        working_dir: job.working_dir,
        env_vars: job.env_vars,
        allow_concurrent: job.allow_concurrent,
        on_failure: FailurePolicy::Abort,
        last_run_at: job.last_run_at,
        last_run_status,
        last_run_id: None,
        next_run_at: None,
        created_at: job.created_at,
        updated_at: job.updated_at,
    }
}

/// Infer a `script_type` string from a script file extension.
///
/// Returns `None` if the extension is not recognised so callers can fall back
/// to their own defaults.
fn script_type_from_path(path: &str) -> Option<String> {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())?;
    match ext.to_lowercase().as_str() {
        "sh" | "bash" => Some("shell".to_string()),
        "bat" | "cmd" => Some("batch".to_string()),
        "py" => Some("python".to_string()),
        "ps1" => Some("powershell".to_string()),
        _ => None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::migration::legacy_types::ExecutionType;
    use crate::models::workflow::{RunStatus, ScheduleMode, StepDef};
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn make_job(name: &str) -> Job {
        let now = Utc::now();
        Job {
            id: Uuid::now_v7(),
            name: name.to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: Some("America/New_York".to_string()),
            working_dir: Some("/tmp".to_string()),
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            allow_concurrent: false,
            schedule_mode: ScheduleMode::default(),
            pre_hook: None,
            post_hook: None,
            pre_hook_script_type: None,
            post_hook_script_type: None,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        }
    }

    // ── synthesise_workflow round-trip ────────────────────────────────────────

    #[test]
    fn test_basic_shell_job_synthesises_main_step_only() {
        let job = make_job("my-job");
        let wf = synthesise_workflow(job.clone());

        assert_eq!(wf.id, job.id, "id must be preserved");
        assert_eq!(wf.name, job.name);
        assert_eq!(wf.schedule, job.schedule);
        assert_eq!(wf.timezone, job.timezone);
        assert_eq!(wf.enabled, job.enabled);
        assert_eq!(wf.version, 1);
        assert_eq!(wf.steps.len(), 1, "no hooks → only main step");

        match &wf.steps[0] {
            StepDef::Shell(s) => {
                assert_eq!(s.common.id, "main");
                assert_eq!(s.command, "echo hello");
                assert!(s.common.timeout_secs.is_none(), "timeout_secs=0 → None");
            }
            other => panic!("Expected Shell step, got: {:?}", other),
        }
    }

    #[test]
    fn test_script_file_job_synthesises_script_step() {
        let mut job = make_job("script-job");
        job.execution = ExecutionType::ScriptFile("/usr/local/bin/deploy.sh".to_string());
        let wf = synthesise_workflow(job);

        assert_eq!(wf.steps.len(), 1);
        match &wf.steps[0] {
            StepDef::Script(s) => {
                assert_eq!(s.common.id, "main");
                assert_eq!(s.path, "/usr/local/bin/deploy.sh");
                assert_eq!(s.script_type.as_deref(), Some("shell"));
            }
            other => panic!("Expected Script step, got: {:?}", other),
        }
    }

    #[test]
    fn test_pre_and_post_hooks_synthesised_as_shell_steps() {
        let mut job = make_job("hooked-job");
        job.pre_hook = Some("setup.sh".to_string());
        job.post_hook = Some("cleanup.sh".to_string());
        let wf = synthesise_workflow(job);

        assert_eq!(wf.steps.len(), 3);

        // pre_hook
        match &wf.steps[0] {
            StepDef::Shell(s) => {
                assert_eq!(s.common.id, "pre_hook");
                assert_eq!(s.command, "setup.sh");
                assert!(!s.common.always_run);
            }
            other => panic!("Expected Shell step for pre_hook, got: {:?}", other),
        }

        // main
        match &wf.steps[1] {
            StepDef::Shell(s) => {
                assert_eq!(s.common.id, "main");
            }
            other => panic!("Expected Shell step for main, got: {:?}", other),
        }

        // post_hook
        match &wf.steps[2] {
            StepDef::Shell(s) => {
                assert_eq!(s.common.id, "post_hook");
                assert_eq!(s.command, "cleanup.sh");
                assert!(s.common.always_run, "post_hook must have always_run=true");
            }
            other => panic!("Expected Shell step for post_hook, got: {:?}", other),
        }
    }

    #[test]
    fn test_timeout_propagated_to_all_steps() {
        let mut job = make_job("timeout-job");
        job.timeout_secs = 120;
        job.pre_hook = Some("pre.sh".to_string());
        job.post_hook = Some("post.sh".to_string());
        let wf = synthesise_workflow(job);

        for step in &wf.steps {
            let common = match step {
                StepDef::Shell(s) => &s.common,
                StepDef::Script(s) => &s.common,
                _ => panic!("Unexpected step kind"),
            };
            assert_eq!(
                common.timeout_secs,
                Some(120),
                "step {} should have timeout_secs=120",
                common.id
            );
        }
    }

    #[test]
    fn test_timeout_zero_becomes_none() {
        let mut job = make_job("no-timeout-job");
        job.timeout_secs = 0;
        let wf = synthesise_workflow(job);

        match &wf.steps[0] {
            StepDef::Shell(s) => {
                assert!(s.common.timeout_secs.is_none());
            }
            _ => panic!("Expected Shell step"),
        }
    }

    #[test]
    fn test_last_exit_code_maps_to_run_status() {
        let mut job = make_job("success-job");
        job.last_exit_code = Some(0);
        let wf = synthesise_workflow(job);
        assert_eq!(wf.last_run_status, Some(RunStatus::Completed));

        let mut job2 = make_job("fail-job");
        job2.last_exit_code = Some(1);
        let wf2 = synthesise_workflow(job2);
        assert_eq!(wf2.last_run_status, Some(RunStatus::Failed));

        let mut job3 = make_job("never-run-job");
        job3.last_exit_code = None;
        let wf3 = synthesise_workflow(job3);
        assert!(wf3.last_run_status.is_none());
    }

    #[test]
    fn test_allow_concurrent_preserved() {
        let mut job = make_job("concurrent-job");
        job.allow_concurrent = true;
        let wf = synthesise_workflow(job);
        assert!(wf.allow_concurrent);

        let mut job2 = make_job("serial-job");
        job2.allow_concurrent = false;
        let wf2 = synthesise_workflow(job2);
        assert!(!wf2.allow_concurrent);
    }

    #[test]
    fn test_script_type_inferred_from_extension() {
        assert_eq!(
            script_type_from_path("deploy.sh"),
            Some("shell".to_string())
        );
        assert_eq!(
            script_type_from_path("deploy.bash"),
            Some("shell".to_string())
        );
        assert_eq!(script_type_from_path("run.bat"), Some("batch".to_string()));
        assert_eq!(script_type_from_path("run.cmd"), Some("batch".to_string()));
        assert_eq!(
            script_type_from_path("script.py"),
            Some("python".to_string())
        );
        assert_eq!(
            script_type_from_path("run.ps1"),
            Some("powershell".to_string())
        );
        assert_eq!(script_type_from_path("binary"), None);
        assert_eq!(script_type_from_path("data.json"), None);
    }

    // ── Migration::run integration tests ─────────────────────────────────────

    async fn write_jobs_json(dir: &std::path::Path, jobs: &[Job]) {
        let json = serde_json::to_string_pretty(jobs).unwrap();
        tokio::fs::write(dir.join("jobs.json"), json.as_bytes())
            .await
            .unwrap();
    }

    async fn write_workflows_json(dir: &std::path::Path) {
        tokio::fs::write(dir.join("workflows.json"), b"[]")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_not_needed_when_no_jobs_json() {
        let tmp = TempDir::new().unwrap();
        let m = JobsToWorkflows;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(
            !applied,
            "run() should return false when jobs.json is absent"
        );
    }

    #[tokio::test]
    async fn test_already_applied_when_workflows_json_exists() {
        let tmp = TempDir::new().unwrap();
        write_jobs_json(tmp.path(), &[make_job("j1")]).await;
        write_workflows_json(tmp.path()).await;

        let m = JobsToWorkflows;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(
            !applied,
            "run() should return false when workflows.json already exists"
        );
    }

    #[tokio::test]
    async fn test_run_migrates_and_returns_true() {
        let tmp = TempDir::new().unwrap();
        let jobs = vec![make_job("j1"), make_job("j2"), make_job("j3")];
        write_jobs_json(tmp.path(), &jobs).await;

        let m = JobsToWorkflows;
        let applied = m.run(tmp.path()).await.unwrap();
        assert!(applied, "run() should return true after migrating");
    }

    #[tokio::test]
    async fn test_workflows_json_written_after_migration() {
        let tmp = TempDir::new().unwrap();
        let job = make_job("migrated-job");
        write_jobs_json(tmp.path(), &[job.clone()]).await;

        let m = JobsToWorkflows;
        m.run(tmp.path()).await.unwrap();

        let wf_path = tmp.path().join("workflows.json");
        assert!(
            wf_path.exists(),
            "workflows.json should exist after migration"
        );

        let content = tokio::fs::read_to_string(&wf_path).await.unwrap();
        let workflows: Vec<Workflow> = serde_json::from_str(&content).unwrap();
        assert_eq!(workflows.len(), 1);
        assert_eq!(
            workflows[0].id, job.id,
            "workflow id must match original job id"
        );
        assert_eq!(workflows[0].name, job.name);
    }

    #[tokio::test]
    async fn test_jobs_json_renamed_to_backup_after_migration() {
        let tmp = TempDir::new().unwrap();
        write_jobs_json(tmp.path(), &[make_job("j1")]).await;

        let m = JobsToWorkflows;
        m.run(tmp.path()).await.unwrap();

        // jobs.json should NOT exist anymore (renamed)
        assert!(
            !tmp.path().join("jobs.json").exists(),
            "jobs.json should be renamed after migration"
        );

        // A backup file should exist
        let backup_exists = std::fs::read_dir(tmp.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("jobs.json.migrated.")
            });
        assert!(
            backup_exists,
            "a jobs.json.migrated.<ts> backup file must exist"
        );
    }

    #[tokio::test]
    async fn test_migration_is_idempotent_second_call_returns_false() {
        let tmp = TempDir::new().unwrap();
        write_jobs_json(tmp.path(), &[make_job("j1")]).await;

        let m = JobsToWorkflows;

        // First call migrates
        let r1 = m.run(tmp.path()).await.unwrap();
        assert!(r1, "first run should return true");

        // Second call: workflows.json now exists → not applied
        let r2 = m.run(tmp.path()).await.unwrap();
        assert!(!r2, "second run should return false (already applied)");
    }

    // ── Non-shell hook script_type tests ──────────────────────────────────────

    #[tokio::test]
    async fn test_pre_hook_python_type_emits_script_step_and_writes_file() {
        let tmp = TempDir::new().unwrap();
        let migrated_scripts_dir = tmp.path().join("migrated_scripts");
        tokio::fs::create_dir_all(&migrated_scripts_dir)
            .await
            .unwrap();

        let mut job = make_job("py-hook-job");
        job.pre_hook = Some("import os\nprint(os.getcwd())".to_string());
        job.pre_hook_script_type = Some("python".to_string());

        let wf = synthesise_workflow_async(job.clone(), &migrated_scripts_dir)
            .await
            .unwrap();

        assert_eq!(wf.steps.len(), 2, "pre_hook ScriptStep + main ShellStep");
        match &wf.steps[0] {
            StepDef::Script(s) => {
                assert_eq!(s.common.id, "pre_hook");
                assert_eq!(s.script_type.as_deref(), Some("python"));
                assert!(
                    s.path.ends_with(".py"),
                    "script path should have .py extension, got: {}",
                    s.path
                );
                assert!(
                    std::path::Path::new(&s.path).exists(),
                    "migrated script file should exist at {}",
                    s.path
                );
                let content = std::fs::read_to_string(&s.path).unwrap();
                assert_eq!(content, "import os\nprint(os.getcwd())");
            }
            other => panic!("Expected Script step for pre_hook, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_post_hook_powershell_type_emits_script_step_with_always_run() {
        let tmp = TempDir::new().unwrap();
        let migrated_scripts_dir = tmp.path().join("migrated_scripts");
        tokio::fs::create_dir_all(&migrated_scripts_dir)
            .await
            .unwrap();

        let mut job = make_job("ps-hook-job");
        job.post_hook = Some("Write-Host 'done'".to_string());
        job.post_hook_script_type = Some("powershell".to_string());

        let wf = synthesise_workflow_async(job.clone(), &migrated_scripts_dir)
            .await
            .unwrap();

        assert_eq!(wf.steps.len(), 2, "main ShellStep + post_hook ScriptStep");
        match &wf.steps[1] {
            StepDef::Script(s) => {
                assert_eq!(s.common.id, "post_hook");
                assert!(s.common.always_run, "post_hook must have always_run=true");
                assert_eq!(s.script_type.as_deref(), Some("powershell"));
                assert!(
                    s.path.ends_with(".ps1"),
                    "script path should have .ps1 extension, got: {}",
                    s.path
                );
                let content = std::fs::read_to_string(&s.path).unwrap();
                assert_eq!(content, "Write-Host 'done'");
            }
            other => panic!("Expected Script step for post_hook, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_shell_hook_with_explicit_shell_type_stays_as_shell_step() {
        let tmp = TempDir::new().unwrap();
        let migrated_scripts_dir = tmp.path().join("migrated_scripts");
        tokio::fs::create_dir_all(&migrated_scripts_dir)
            .await
            .unwrap();

        let mut job = make_job("shell-hook-job");
        job.pre_hook = Some("echo setup".to_string());
        job.pre_hook_script_type = Some("shell".to_string());

        let wf = synthesise_workflow_async(job, &migrated_scripts_dir)
            .await
            .unwrap();

        match &wf.steps[0] {
            StepDef::Shell(s) => {
                assert_eq!(s.common.id, "pre_hook");
                assert_eq!(s.command, "echo setup");
            }
            other => panic!(
                "Expected ShellStep for explicit shell type, got: {:?}",
                other
            ),
        }
    }

    #[tokio::test]
    async fn test_migration_creates_migrated_scripts_dir_for_non_shell_hooks() {
        let tmp = TempDir::new().unwrap();
        let mut job = make_job("py-hook-migration-job");
        job.pre_hook = Some("print('hello')".to_string());
        job.pre_hook_script_type = Some("python".to_string());
        write_jobs_json(tmp.path(), &[job]).await;

        let m = JobsToWorkflows;
        m.run(tmp.path()).await.unwrap();

        let migrated_scripts_dir = tmp.path().join("migrated_scripts");
        assert!(
            migrated_scripts_dir.exists(),
            "migrated_scripts directory should be created during migration"
        );

        let entries: Vec<_> = std::fs::read_dir(&migrated_scripts_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .collect();
        assert!(
            !entries.is_empty(),
            "migrated_scripts directory should contain the hook script file"
        );
    }
}
