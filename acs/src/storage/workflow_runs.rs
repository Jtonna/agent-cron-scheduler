use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::WorkflowRun;

// ─── WorkflowRunStore trait ───────────────────────────────────────────────────

#[async_trait]
pub trait WorkflowRunStore: Send + Sync {
    /// Create a new run record (status=Running, started_at=now).
    async fn create_run(&self, run: WorkflowRun) -> Result<(), AcsError>;

    /// Update an existing run record. Atomic (write-rename).
    async fn update_run(&self, run: &WorkflowRun) -> Result<(), AcsError>;

    /// Get a single run by id (uses index to find the workflow_id directory).
    async fn get_run(&self, run_id: Uuid) -> Result<Option<WorkflowRun>, AcsError>;

    /// List runs for a specific workflow, latest-first. Pagination via limit + offset.
    /// limit=0 means "no limit" (return all).
    async fn list_runs(
        &self,
        workflow_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowRun>, AcsError>;

    /// Total count of runs for a workflow.
    async fn count_runs(&self, workflow_id: Uuid) -> Result<usize, AcsError>;

    /// Delete a run (and its log file). Best-effort.
    async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError>;
}

// ─── FsWorkflowRunStore ───────────────────────────────────────────────────────

/// Index file mapping run_id → workflow_id for O(1) `get_run` without scanning.
type RunIndex = HashMap<Uuid, Uuid>;

pub struct FsWorkflowRunStore {
    /// <data_dir>/runs/
    runs_dir: PathBuf,
    /// In-memory cache of run_id → workflow_id, kept in sync with the index file.
    index: Arc<Mutex<RunIndex>>,
}

impl FsWorkflowRunStore {
    /// Create a new FsWorkflowRunStore.
    ///
    /// Creates `<data_dir>/runs/` if it does not exist.
    /// Loads the index file (`<data_dir>/runs/index.json`) if present.
    /// On index corruption, logs a warning and falls back to a directory scan.
    pub async fn new(data_dir: &Path) -> Result<Self> {
        let runs_dir = data_dir.join("runs");
        tokio::fs::create_dir_all(&runs_dir)
            .await
            .context("Failed to create runs directory")?;

        let index = Self::load_or_rebuild_index(&runs_dir).await?;

        Ok(Self {
            runs_dir,
            index: Arc::new(Mutex::new(index)),
        })
    }

    // ── Index helpers ─────────────────────────────────────────────────────────

    fn index_path(runs_dir: &Path) -> PathBuf {
        runs_dir.join("index.json")
    }

    async fn load_or_rebuild_index(runs_dir: &Path) -> Result<RunIndex> {
        let path = Self::index_path(runs_dir);
        if path.exists() {
            let content = tokio::fs::read_to_string(&path)
                .await
                .context("Failed to read index.json")?;
            match serde_json::from_str::<RunIndex>(&content) {
                Ok(index) => return Ok(index),
                Err(e) => {
                    tracing::warn!(
                        "runs/index.json is corrupted ({}), rebuilding from directory scan",
                        e
                    );
                    let ts = Utc::now().timestamp();
                    let backup = runs_dir.join(format!("index.json.bak.{}", ts));
                    if let Err(be) = tokio::fs::copy(&path, &backup).await {
                        tracing::error!("Failed to backup corrupted index.json: {}", be);
                    }
                }
            }
        }
        // Rebuild by scanning all <workflow_id>/<run_id>.json files.
        Self::rebuild_index_from_scan(runs_dir).await
    }

    async fn rebuild_index_from_scan(runs_dir: &Path) -> Result<RunIndex> {
        let mut index = RunIndex::new();

        let mut wf_dir_entries = tokio::fs::read_dir(runs_dir)
            .await
            .context("Failed to read runs dir")?;

        while let Some(wf_entry) = wf_dir_entries.next_entry().await? {
            let wf_path = wf_entry.path();
            if !wf_path.is_dir() {
                continue;
            }
            let wf_id_str = wf_entry.file_name();
            let wf_id_str = wf_id_str.to_string_lossy();
            let workflow_id = match Uuid::parse_str(&wf_id_str) {
                Ok(id) => id,
                Err(_) => continue,
            };

            let mut run_entries = match tokio::fs::read_dir(&wf_path).await {
                Ok(r) => r,
                Err(_) => continue,
            };
            while let Some(run_entry) = run_entries.next_entry().await? {
                let fname = run_entry.file_name();
                let fname = fname.to_string_lossy();
                if !fname.ends_with(".json") {
                    continue;
                }
                let stem = fname.trim_end_matches(".json");
                if let Ok(run_id) = Uuid::parse_str(stem) {
                    index.insert(run_id, workflow_id);
                }
            }
        }

        Ok(index)
    }

    /// Atomically write the index to disk.
    async fn persist_index(&self, index: &RunIndex) -> Result<(), AcsError> {
        let path = Self::index_path(&self.runs_dir);
        let tmp_path = self.runs_dir.join("index.json.tmp");
        let json = serde_json::to_string(index)
            .map_err(|e| AcsError::Storage(format!("Failed to serialize index: {}", e)))?;
        tokio::fs::write(&tmp_path, json.as_bytes())
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to write index.tmp: {}", e)))?;
        tokio::fs::rename(&tmp_path, &path)
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to rename index.tmp: {}", e)))?;
        Ok(())
    }

    // ── File helpers ──────────────────────────────────────────────────────────

    fn run_file_path(&self, workflow_id: Uuid, run_id: Uuid) -> PathBuf {
        self.runs_dir
            .join(workflow_id.to_string())
            .join(format!("{}.json", run_id))
    }

    async fn write_run_atomic(&self, workflow_id: Uuid, run: &WorkflowRun) -> Result<(), AcsError> {
        let dir = self.runs_dir.join(workflow_id.to_string());
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to create run dir: {}", e)))?;

        let run_path = dir.join(format!("{}.json", run.run_id));
        let tmp_path = dir.join(format!("{}.json.tmp", run.run_id));

        let json = serde_json::to_string_pretty(run)
            .map_err(|e| AcsError::Storage(format!("Failed to serialize run: {}", e)))?;

        tokio::fs::write(&tmp_path, json.as_bytes())
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to write run.tmp: {}", e)))?;

        tokio::fs::rename(&tmp_path, &run_path)
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to rename run.tmp: {}", e)))?;

        Ok(())
    }

    /// Enumerate all run JSON files in `<runs_dir>/<workflow_id>/` and return
    /// them sorted by run_id descending (latest Uuid v7 first = latest-first).
    async fn list_run_files_for_workflow(&self, workflow_id: Uuid) -> Result<Vec<Uuid>, AcsError> {
        let dir = self.runs_dir.join(workflow_id.to_string());
        if !dir.exists() {
            return Ok(vec![]);
        }

        let mut entries = tokio::fs::read_dir(&dir)
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to read workflow run dir: {}", e)))?;

        let mut run_ids: Vec<Uuid> = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AcsError::Storage(e.to_string()))?
        {
            let fname = entry.file_name();
            let fname = fname.to_string_lossy();
            if !fname.ends_with(".json") {
                continue;
            }
            let stem = fname.trim_end_matches(".json");
            if let Ok(run_id) = Uuid::parse_str(stem) {
                run_ids.push(run_id);
            }
        }

        // Sort descending (Uuid v7 is time-ordered; reverse = latest first)
        run_ids.sort_by(|a, b| b.cmp(a));
        Ok(run_ids)
    }
}

#[async_trait]
impl WorkflowRunStore for FsWorkflowRunStore {
    async fn create_run(&self, run: WorkflowRun) -> Result<(), AcsError> {
        let workflow_id = run.workflow_id;
        let run_id = run.run_id;

        self.write_run_atomic(workflow_id, &run).await?;

        // Update in-memory index and persist it.
        let mut index = self.index.lock().await;
        index.insert(run_id, workflow_id);
        self.persist_index(&index).await?;

        Ok(())
    }

    async fn update_run(&self, run: &WorkflowRun) -> Result<(), AcsError> {
        let workflow_id = {
            let index = self.index.lock().await;
            match index.get(&run.run_id).copied() {
                Some(id) => id,
                None => {
                    return Err(AcsError::NotFound(format!(
                        "Run '{}' not found in index",
                        run.run_id
                    )));
                }
            }
        };
        self.write_run_atomic(workflow_id, run).await
    }

    async fn get_run(&self, run_id: Uuid) -> Result<Option<WorkflowRun>, AcsError> {
        let workflow_id = {
            let index = self.index.lock().await;
            match index.get(&run_id).copied() {
                Some(id) => id,
                None => return Ok(None),
            }
        };

        let path = self.run_file_path(workflow_id, run_id);
        if !path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| AcsError::Storage(format!("Failed to read run file: {}", e)))?;

        let run = serde_json::from_str::<WorkflowRun>(&content)
            .map_err(|e| AcsError::Storage(format!("Failed to parse run file: {}", e)))?;

        Ok(Some(run))
    }

    async fn list_runs(
        &self,
        workflow_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowRun>, AcsError> {
        let run_ids = self.list_run_files_for_workflow(workflow_id).await?;

        let paginated: Vec<Uuid> = if limit == 0 {
            run_ids.into_iter().skip(offset).collect()
        } else {
            run_ids.into_iter().skip(offset).take(limit).collect()
        };

        let mut runs = Vec::with_capacity(paginated.len());
        for run_id in paginated {
            let path = self.run_file_path(workflow_id, run_id);
            if !path.exists() {
                continue;
            }
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| AcsError::Storage(format!("Failed to read run file: {}", e)))?;
            match serde_json::from_str::<WorkflowRun>(&content) {
                Ok(run) => runs.push(run),
                Err(e) => {
                    tracing::warn!("Skipping corrupted run file {}: {}", path.display(), e);
                }
            }
        }

        Ok(runs)
    }

    async fn count_runs(&self, workflow_id: Uuid) -> Result<usize, AcsError> {
        let run_ids = self.list_run_files_for_workflow(workflow_id).await?;
        Ok(run_ids.len())
    }

    async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError> {
        let workflow_id = {
            let mut index = self.index.lock().await;
            match index.remove(&run_id) {
                Some(id) => {
                    // Persist the updated index immediately.
                    if let Err(e) = self.persist_index(&index).await {
                        // Re-insert so index stays consistent; best-effort.
                        index.insert(run_id, id);
                        return Err(e);
                    }
                    id
                }
                None => {
                    // Not in index — nothing to delete.
                    return Ok(());
                }
            }
        };

        // Remove the JSON file (best-effort).
        let run_path = self.run_file_path(workflow_id, run_id);
        if run_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&run_path).await {
                tracing::warn!(
                    "Failed to remove run file {} (best-effort): {}",
                    run_path.display(),
                    e
                );
            }
        }

        // Remove the matching .log file if present (best-effort).
        // Logs live at <data_dir>/logs/<workflow_id>/<run_id>.log, not alongside the JSON.
        let logs_dir = self
            .runs_dir
            .parent()
            .unwrap_or(&self.runs_dir)
            .join("logs");
        let log_path = logs_dir
            .join(workflow_id.to_string())
            .join(format!("{}.log", run_id));
        if log_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&log_path).await {
                tracing::warn!(
                    "Failed to remove run log {} (best-effort): {}",
                    log_path.display(),
                    e
                );
            }
        }

        Ok(())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, RunStatus, ScheduleMode, ShellStep, StepDef, StepDefCommon,
        Workflow, WorkflowRun,
    };
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    // ── Helpers ───────────────────────────────────────────────────────────────

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

    fn make_workflow(workflow_id: Uuid) -> Workflow {
        let now = Utc::now();
        Workflow {
            id: workflow_id,
            name: "test-workflow".to_string(),
            version: 1,
            schedule: "*/5 * * * *".to_string(),
            timezone: None,
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            input_schema: None,
            default_input: None,
            working_dir: None,
            env_vars: None,
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

    fn make_run(workflow_id: Uuid) -> WorkflowRun {
        let wf = make_workflow(workflow_id);
        WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id,
            workflow_version: 1,
            workflow_snapshot: wf,
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            trigger_input: None,
            steps: vec![],
            total_cost_usd: None,
            total_duration_ms: None,
        }
    }

    async fn setup_store() -> (FsWorkflowRunStore, TempDir) {
        let tmp = TempDir::new().expect("create temp dir");
        let store = FsWorkflowRunStore::new(tmp.path())
            .await
            .expect("create store");
        (store, tmp)
    }

    // ── 1. test_create_run_writes_file ────────────────────────────────────────

    #[tokio::test]
    async fn test_create_run_writes_file() {
        let (store, tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();
        let run = make_run(workflow_id);
        let run_id = run.run_id;

        store.create_run(run.clone()).await.expect("create_run");

        // Verify file exists at the correct path.
        let run_file = tmp
            .path()
            .join("runs")
            .join(workflow_id.to_string())
            .join(format!("{}.json", run_id));
        assert!(
            run_file.exists(),
            "Run JSON file should exist at {}",
            run_file.display()
        );

        // Verify content is correct.
        let content = tokio::fs::read_to_string(&run_file).await.expect("read");
        let parsed: WorkflowRun = serde_json::from_str(&content).expect("parse");
        assert_eq!(parsed.run_id, run_id);
        assert_eq!(parsed.workflow_id, workflow_id);
        assert_eq!(parsed.status, RunStatus::Running);
    }

    // ── 2. test_get_run_returns_created_run ───────────────────────────────────

    #[tokio::test]
    async fn test_get_run_returns_created_run() {
        let (store, _tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();
        let run = make_run(workflow_id);
        let run_id = run.run_id;

        store.create_run(run.clone()).await.expect("create_run");

        let fetched = store.get_run(run_id).await.expect("get_run");
        assert!(fetched.is_some(), "Should return the created run");
        let fetched = fetched.unwrap();
        assert_eq!(fetched.run_id, run_id);
        assert_eq!(fetched.workflow_id, workflow_id);
    }

    // ── 3. test_get_run_not_found_returns_none ────────────────────────────────

    #[tokio::test]
    async fn test_get_run_not_found_returns_none() {
        let (store, _tmp) = setup_store().await;
        let result = store.get_run(Uuid::now_v7()).await.expect("get_run");
        assert!(result.is_none(), "Non-existent run_id should return None");
    }

    // ── 4. test_update_run_replaces_atomically ────────────────────────────────

    #[tokio::test]
    async fn test_update_run_replaces_atomically() {
        let (store, tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();
        let mut run = make_run(workflow_id);
        let run_id = run.run_id;

        store.create_run(run.clone()).await.expect("create_run");

        // Update the run status to Completed.
        run.status = RunStatus::Completed;
        run.finished_at = Some(Utc::now());

        store.update_run(&run).await.expect("update_run");

        // Verify no .tmp file was left behind.
        let tmp_file = tmp
            .path()
            .join("runs")
            .join(workflow_id.to_string())
            .join(format!("{}.json.tmp", run_id));
        assert!(
            !tmp_file.exists(),
            "Temp file should not exist after atomic write"
        );

        // Verify the updated content.
        let fetched = store
            .get_run(run_id)
            .await
            .expect("get_run")
            .expect("should exist");
        assert_eq!(fetched.status, RunStatus::Completed);
        assert!(fetched.finished_at.is_some());
    }

    // ── 5. test_list_runs_by_workflow_latest_first ────────────────────────────

    #[tokio::test]
    async fn test_list_runs_by_workflow_latest_first() {
        let (store, _tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();

        // Create 3 runs with a small delay to ensure distinct Uuid v7 ordering.
        let mut run_ids = vec![];
        for _ in 0..3 {
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
            let run = make_run(workflow_id);
            run_ids.push(run.run_id);
            store.create_run(run).await.expect("create_run");
        }

        let listed = store.list_runs(workflow_id, 0, 0).await.expect("list_runs");
        assert_eq!(listed.len(), 3, "Should list all 3 runs");

        // Verify latest-first ordering (run_ids were created in order 0,1,2).
        // listed[0] should have the latest run_id (run_ids[2]).
        assert_eq!(
            listed[0].run_id, run_ids[2],
            "First in list should be the latest run"
        );
        assert_eq!(
            listed[2].run_id, run_ids[0],
            "Last in list should be the oldest run"
        );
    }

    // ── 6. test_list_runs_pagination_offset_limit ─────────────────────────────

    #[tokio::test]
    async fn test_list_runs_pagination_offset_limit() {
        let (store, _tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();

        let mut all_run_ids = vec![];
        for _ in 0..5 {
            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
            let run = make_run(workflow_id);
            all_run_ids.push(run.run_id);
            store.create_run(run).await.expect("create_run");
        }
        // all_run_ids[0] is oldest, all_run_ids[4] is newest.
        // In latest-first order: [4, 3, 2, 1, 0].
        // offset=2, limit=2 → [2, 1].

        let listed = store.list_runs(workflow_id, 2, 2).await.expect("list_runs");
        assert_eq!(listed.len(), 2, "Should return exactly 2 runs");
        assert_eq!(
            listed[0].run_id, all_run_ids[2],
            "First result at offset=2 should be all_run_ids[2]"
        );
        assert_eq!(
            listed[1].run_id, all_run_ids[1],
            "Second result should be all_run_ids[1]"
        );
    }

    // ── 7. test_count_runs ────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_count_runs() {
        let (store, _tmp) = setup_store().await;
        let workflow_a = Uuid::now_v7();
        let workflow_b = Uuid::now_v7();

        for _ in 0..5 {
            store
                .create_run(make_run(workflow_a))
                .await
                .expect("create_run a");
        }
        for _ in 0..2 {
            store
                .create_run(make_run(workflow_b))
                .await
                .expect("create_run b");
        }

        assert_eq!(store.count_runs(workflow_a).await.expect("count_runs a"), 5);
        assert_eq!(store.count_runs(workflow_b).await.expect("count_runs b"), 2);
    }

    // ── 8. test_delete_run_removes_file ───────────────────────────────────────

    #[tokio::test]
    async fn test_delete_run_removes_file() {
        let (store, tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();
        let run = make_run(workflow_id);
        let run_id = run.run_id;

        store.create_run(run).await.expect("create_run");

        // Verify it exists before delete.
        assert!(store.get_run(run_id).await.expect("get").is_some());

        store.delete_run(run_id).await.expect("delete_run");

        // File should be gone.
        let run_file = tmp
            .path()
            .join("runs")
            .join(workflow_id.to_string())
            .join(format!("{}.json", run_id));
        assert!(
            !run_file.exists(),
            "Run file should be removed after delete"
        );

        // Index should no longer contain the run.
        assert!(
            store
                .get_run(run_id)
                .await
                .expect("get after delete")
                .is_none(),
            "get_run should return None after delete"
        );
    }

    // ── 9. test_delete_run_removes_log_file ──────────────────────────────────

    #[tokio::test]
    async fn test_delete_run_removes_log_file() {
        let (store, tmp) = setup_store().await;
        let workflow_id = Uuid::now_v7();
        let run = make_run(workflow_id);
        let run_id = run.run_id;

        store.create_run(run).await.expect("create_run");

        // Create a matching log file at the canonical logs/<workflow_id>/<run_id>.log path.
        let log_dir = tmp.path().join("logs").join(workflow_id.to_string());
        tokio::fs::create_dir_all(&log_dir)
            .await
            .expect("create log dir");
        let log_file = log_dir.join(format!("{}.log", run_id));
        tokio::fs::write(&log_file, b"step output here")
            .await
            .expect("write log file");
        assert!(log_file.exists(), "log file should exist before delete");

        // Verify JSON record exists.
        let run_file = tmp
            .path()
            .join("runs")
            .join(workflow_id.to_string())
            .join(format!("{}.json", run_id));
        assert!(
            run_file.exists(),
            "run JSON file should exist before delete"
        );

        store.delete_run(run_id).await.expect("delete_run");

        // Both JSON record AND log file should be gone.
        assert!(
            !run_file.exists(),
            "Run JSON file should be removed after delete"
        );
        assert!(
            !log_file.exists(),
            "Run log file at logs/<workflow_id>/<run_id>.log should be removed after delete"
        );

        // Index should no longer contain the run.
        assert!(
            store
                .get_run(run_id)
                .await
                .expect("get after delete")
                .is_none(),
            "get_run should return None after delete"
        );
    }

    // ── 10. test_index_corruption_handled ────────────────────────────────────

    #[tokio::test]
    async fn test_index_corruption_handled() {
        let tmp = TempDir::new().expect("create temp dir");
        let runs_dir = tmp.path().join("runs");
        tokio::fs::create_dir_all(&runs_dir)
            .await
            .expect("create runs dir");

        // Write corrupted index.json.
        let index_path = runs_dir.join("index.json");
        tokio::fs::write(&index_path, b"not valid json {{{{")
            .await
            .expect("write corrupted index");

        // Pre-populate a run file so scan can rebuild.
        let workflow_id = Uuid::now_v7();
        let run = {
            let wf = make_workflow(workflow_id);
            WorkflowRun {
                run_id: Uuid::now_v7(),
                workflow_id,
                workflow_version: 1,
                workflow_snapshot: wf,
                started_at: Utc::now(),
                finished_at: None,
                status: RunStatus::Running,
                trigger_input: None,
                steps: vec![],
                total_cost_usd: None,
                total_duration_ms: None,
            }
        };
        let run_id = run.run_id;
        let wf_dir = runs_dir.join(workflow_id.to_string());
        tokio::fs::create_dir_all(&wf_dir)
            .await
            .expect("create wf dir");
        tokio::fs::write(
            wf_dir.join(format!("{}.json", run_id)),
            serde_json::to_string_pretty(&run).unwrap().as_bytes(),
        )
        .await
        .expect("write run file");

        // Opening the store should rebuild the index from the scan.
        let store = FsWorkflowRunStore::new(tmp.path())
            .await
            .expect("create store despite corrupted index");

        // The rebuild should have found the pre-populated run.
        let fetched = store.get_run(run_id).await.expect("get_run");
        assert!(
            fetched.is_some(),
            "Store should have rebuilt index from scan and find the pre-populated run"
        );

        // A backup of the corrupted index should exist.
        let entries: Vec<_> = std::fs::read_dir(&runs_dir)
            .expect("read dir")
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("index.json.bak.")
            })
            .collect();
        assert!(
            !entries.is_empty(),
            "A backup of the corrupted index should exist"
        );
    }

    // ── 11. test_persistence_across_instances ────────────────────────────────

    #[tokio::test]
    async fn test_persistence_across_instances() {
        let tmp = TempDir::new().expect("create temp dir");
        let workflow_id = Uuid::now_v7();
        let run_id;

        {
            let store = FsWorkflowRunStore::new(tmp.path())
                .await
                .expect("create store 1");
            let run = make_run(workflow_id);
            run_id = run.run_id;
            store.create_run(run).await.expect("create_run");
        }

        {
            let store = FsWorkflowRunStore::new(tmp.path())
                .await
                .expect("create store 2");
            let fetched = store.get_run(run_id).await.expect("get_run");
            assert!(
                fetched.is_some(),
                "Run should persist across store instances"
            );
            assert_eq!(fetched.unwrap().run_id, run_id);
        }
    }
}
