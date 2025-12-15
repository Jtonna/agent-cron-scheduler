use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::job::{validate_job_update, validate_new_job};
use crate::models::{Job, JobUpdate, NewJob};
use crate::storage::JobStore;

pub struct JsonJobStore {
    file_path: PathBuf,
    cache: RwLock<Vec<Job>>,
}

impl JsonJobStore {
    /// Create a new JsonJobStore, loading existing data from disk if present.
    ///
    /// If `jobs.json` is corrupted (invalid JSON), creates a backup at
    /// `jobs.json.bak`, logs a warning, and starts with an empty job list.
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        tokio::fs::create_dir_all(&data_dir)
            .await
            .context("Failed to create data directory")?;

        let file_path = data_dir.join("jobs.json");

        let jobs = if file_path.exists() {
            let content = tokio::fs::read_to_string(&file_path)
                .await
                .context("Failed to read jobs.json")?;
            match serde_json::from_str::<Vec<Job>>(&content) {
                Ok(parsed) => parsed,
                Err(e) => {
                    // Corrupted JSON: create backup and start with empty list
                    tracing::warn!(
                        "jobs.json is corrupted ({}), creating backup and starting empty",
                        e
                    );
                    let backup_path = data_dir.join("jobs.json.bak");
                    if let Err(backup_err) = tokio::fs::copy(&file_path, &backup_path).await {
                        tracing::error!(
                            "Failed to create backup of corrupted jobs.json: {}",
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
            file_path,
            cache: RwLock::new(jobs),
        })
    }

    /// Atomically write the jobs cache to disk.
    /// Writes to a .tmp file first, then renames to the actual file.
    async fn persist(&self, jobs: &[Job]) -> Result<()> {
        let tmp_path = self.file_path.with_extension("json.tmp");

        let json = serde_json::to_string_pretty(jobs).context("Failed to serialize jobs")?;

        tokio::fs::write(&tmp_path, json.as_bytes())
            .await
            .context("Failed to write temporary jobs file")?;

        tokio::fs::rename(&tmp_path, &self.file_path)
            .await
            .context("Failed to rename temporary jobs file")?;

        Ok(())
    }
}

#[async_trait]
impl JobStore for JsonJobStore {
    async fn list_jobs(&self) -> Result<Vec<Job>> {
        let cache = self.cache.read().await;
        Ok(cache.clone())
    }

    async fn get_job(&self, id: Uuid) -> Result<Option<Job>> {
        let cache = self.cache.read().await;
        Ok(cache.iter().find(|j| j.id == id).cloned())
    }

    async fn find_by_name(&self, name: &str) -> Result<Option<Job>> {
        let cache = self.cache.read().await;
        Ok(cache.iter().find(|j| j.name == name).cloned())
    }

    async fn create_job(&self, new: NewJob) -> Result<Job> {
        validate_new_job(&new)?;

        let mut cache = self.cache.write().await;

        // Check for duplicate name
        if cache.iter().any(|j| j.name == new.name) {
            return Err(AcsError::Conflict(format!(
                "A job with name '{}' already exists",
                new.name
            ))
            .into());
        }

        let now = Utc::now();
        let job = Job {
            id: Uuid::now_v7(),
            name: new.name,
            schedule: new.schedule,
            execution: new.execution,
            enabled: new.enabled,
            timezone: new.timezone,
            working_dir: new.working_dir,
            env_vars: new.env_vars,
            timeout_secs: new.timeout_secs,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };

        cache.push(job.clone());
        self.persist(&cache).await?;

        Ok(job)
    }

    async fn update_job(&self, id: Uuid, update: JobUpdate) -> Result<Job> {
        validate_job_update(&update)?;

        let mut cache = self.cache.write().await;

        let idx = cache
            .iter()
            .position(|j| j.id == id)
            .ok_or_else(|| AcsError::NotFound(format!("Job with id '{}' not found", id)))?;

        // Check for duplicate name (excluding self)
        if let Some(ref new_name) = update.name {
            if cache.iter().any(|j| j.name == *new_name && j.id != id) {
                return Err(AcsError::Conflict(format!(
                    "A job with name '{}' already exists",
                    new_name
                ))
                .into());
            }
        }

        let job = &mut cache[idx];

        if let Some(name) = update.name {
            job.name = name;
        }
        if let Some(schedule) = update.schedule {
            job.schedule = schedule;
        }
        if let Some(execution) = update.execution {
            job.execution = execution;
        }
        if let Some(enabled) = update.enabled {
            job.enabled = enabled;
        }
        if let Some(timezone) = update.timezone {
            job.timezone = Some(timezone);
        }
        if let Some(working_dir) = update.working_dir {
            job.working_dir = Some(working_dir);
        }
        if let Some(env_vars) = update.env_vars {
            job.env_vars = Some(env_vars);
        }
        if let Some(timeout_secs) = update.timeout_secs {
            job.timeout_secs = timeout_secs;
        }
        // Internal metadata fields (not user-editable, set by the daemon)
        if let Some(last_run_at) = update.last_run_at {
            job.last_run_at = last_run_at;
        }
        if let Some(last_exit_code) = update.last_exit_code {
            job.last_exit_code = last_exit_code;
        }
        job.updated_at = Utc::now();

        let updated_job = job.clone();
        self.persist(&cache).await?;

        Ok(updated_job)
    }

    async fn delete_job(&self, id: Uuid) -> Result<()> {
        let mut cache = self.cache.write().await;

        let idx = cache
            .iter()
            .position(|j| j.id == id)
            .ok_or_else(|| AcsError::NotFound(format!("Job with id '{}' not found", id)))?;

        cache.remove(idx);
        self.persist(&cache).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ExecutionType;
    use tempfile::TempDir;

    fn make_new_job(name: &str) -> NewJob {
        NewJob {
            name: name.to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
        }
    }

    async fn setup_store() -> (JsonJobStore, TempDir) {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let store = JsonJobStore::new(tmp_dir.path().to_path_buf())
            .await
            .expect("create store");
        (store, tmp_dir)
    }

    #[tokio::test]
    async fn test_create_job() {
        let (store, _tmp) = setup_store().await;
        let job = store
            .create_job(make_new_job("test-job"))
            .await
            .expect("create");
        assert_eq!(job.name, "test-job");
        assert_eq!(job.schedule, "*/5 * * * *");
        assert!(job.enabled);
        assert!(job.last_run_at.is_none());
        assert!(job.last_exit_code.is_none());
    }

    #[tokio::test]
    async fn test_get_job() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_job(make_new_job("test-job"))
            .await
            .expect("create");
        let fetched = store
            .get_job(created.id)
            .await
            .expect("get")
            .expect("found");
        assert_eq!(created.id, fetched.id);
        assert_eq!(created.name, fetched.name);
    }

    #[tokio::test]
    async fn test_get_job_not_found() {
        let (store, _tmp) = setup_store().await;
        let result = store.get_job(Uuid::now_v7()).await.expect("get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_list_jobs() {
        let (store, _tmp) = setup_store().await;
        store
            .create_job(make_new_job("job-1"))
            .await
            .expect("create");
        store
            .create_job(make_new_job("job-2"))
            .await
            .expect("create");
        store
            .create_job(make_new_job("job-3"))
            .await
            .expect("create");
        let jobs = store.list_jobs().await.expect("list");
        assert_eq!(jobs.len(), 3);
    }

    #[tokio::test]
    async fn test_list_jobs_empty() {
        let (store, _tmp) = setup_store().await;
        let jobs = store.list_jobs().await.expect("list");
        assert!(jobs.is_empty());
    }

    #[tokio::test]
    async fn test_update_job() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_job(make_new_job("test-job"))
            .await
            .expect("create");

        let update = JobUpdate {
            name: Some("updated-job".to_string()),
            schedule: Some("0 * * * *".to_string()),
            ..Default::default()
        };

        let updated = store.update_job(created.id, update).await.expect("update");
        assert_eq!(updated.name, "updated-job");
        assert_eq!(updated.schedule, "0 * * * *");
        assert!(
            updated.updated_at > created.updated_at || updated.updated_at == created.updated_at
        );
    }

    #[tokio::test]
    async fn test_update_job_not_found() {
        let (store, _tmp) = setup_store().await;
        let update = JobUpdate {
            name: Some("new-name".to_string()),
            ..Default::default()
        };
        let result = store.update_job(Uuid::now_v7(), update).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_job() {
        let (store, _tmp) = setup_store().await;
        let created = store
            .create_job(make_new_job("test-job"))
            .await
            .expect("create");
        store.delete_job(created.id).await.expect("delete");
        let result = store.get_job(created.id).await.expect("get");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete_job_not_found() {
        let (store, _tmp) = setup_store().await;
        let result = store.delete_job(Uuid::now_v7()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_find_by_name() {
        let (store, _tmp) = setup_store().await;
        store
            .create_job(make_new_job("find-me"))
            .await
            .expect("create");
        store
            .create_job(make_new_job("not-me"))
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

    #[tokio::test]
    async fn test_duplicate_name_returns_error() {
        let (store, _tmp) = setup_store().await;
        store
            .create_job(make_new_job("dup-name"))
            .await
            .expect("create first");
        let result = store.create_job(make_new_job("dup-name")).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("already exists") || err_str.contains("Conflict"),
            "Expected conflict error, got: {}",
            err_str
        );
    }

    #[tokio::test]
    async fn test_update_duplicate_name_returns_error() {
        let (store, _tmp) = setup_store().await;
        store
            .create_job(make_new_job("job-a"))
            .await
            .expect("create");
        let job_b = store
            .create_job(make_new_job("job-b"))
            .await
            .expect("create");
        let update = JobUpdate {
            name: Some("job-a".to_string()),
            ..Default::default()
        };
        let result = store.update_job(job_b.id, update).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_atomic_write_produces_valid_json() {
        let (store, tmp) = setup_store().await;
        store
            .create_job(make_new_job("persist-test"))
            .await
            .expect("create");

        // Read the file directly and verify it's valid JSON
        let file_path = tmp.path().join("jobs.json");
        let content = tokio::fs::read_to_string(&file_path)
            .await
            .expect("read file");
        let jobs: Vec<Job> = serde_json::from_str(&content).expect("parse JSON");
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].name, "persist-test");
    }

    #[tokio::test]
    async fn test_persistence_across_instances() {
        let tmp_dir = TempDir::new().expect("create temp dir");

        // Create a store and add a job
        {
            let store = JsonJobStore::new(tmp_dir.path().to_path_buf())
                .await
                .expect("create store");
            store
                .create_job(make_new_job("persistent-job"))
                .await
                .expect("create");
        }

        // Create a new store from the same directory and verify the job is there
        {
            let store = JsonJobStore::new(tmp_dir.path().to_path_buf())
                .await
                .expect("create store");
            let jobs = store.list_jobs().await.expect("list");
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].name, "persistent-job");
        }
    }

    #[tokio::test]
    async fn test_no_tmp_file_left_after_write() {
        let (store, tmp) = setup_store().await;
        store
            .create_job(make_new_job("clean-write"))
            .await
            .expect("create");

        let tmp_file = tmp.path().join("jobs.json.tmp");
        assert!(
            !tmp_file.exists(),
            "Temporary file should not remain after write"
        );
    }

    // --- Phase 8: Corrupted jobs.json recovery tests ---

    #[tokio::test]
    async fn test_corrupted_jobs_json_recovers_empty() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let jobs_file = tmp_dir.path().join("jobs.json");

        // Write corrupted JSON
        tokio::fs::write(&jobs_file, b"this is not valid JSON{{{")
            .await
            .expect("write corrupted file");

        // Should not panic -- recovers with empty list
        let store = JsonJobStore::new(tmp_dir.path().to_path_buf())
            .await
            .expect("create store from corrupted file");

        let jobs = store.list_jobs().await.expect("list");
        assert!(
            jobs.is_empty(),
            "Should start with empty jobs after corruption"
        );
    }

    #[tokio::test]
    async fn test_corrupted_jobs_json_creates_backup() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let jobs_file = tmp_dir.path().join("jobs.json");
        let backup_file = tmp_dir.path().join("jobs.json.bak");

        let corrupted_content = b"corrupted data!!!";
        tokio::fs::write(&jobs_file, corrupted_content)
            .await
            .expect("write corrupted file");

        let _store = JsonJobStore::new(tmp_dir.path().to_path_buf())
            .await
            .expect("create store");

        // Verify backup was created
        assert!(backup_file.exists(), "Backup file should have been created");

        // Verify backup contains the original corrupted content
        let backup_content = tokio::fs::read(&backup_file).await.expect("read backup");
        assert_eq!(
            backup_content, corrupted_content,
            "Backup should contain the original corrupted data"
        );
    }

    #[tokio::test]
    async fn test_corrupted_jobs_json_can_still_create_jobs() {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let jobs_file = tmp_dir.path().join("jobs.json");

        // Write corrupted JSON
        tokio::fs::write(&jobs_file, b"not json")
            .await
            .expect("write corrupted file");

        let store = JsonJobStore::new(tmp_dir.path().to_path_buf())
            .await
            .expect("create store");

        // Should be able to create new jobs after recovery
        let job = store
            .create_job(make_new_job("new-after-corrupt"))
            .await
            .expect("create");
        assert_eq!(job.name, "new-after-corrupt");

        // Verify persistence works
        let jobs = store.list_jobs().await.expect("list");
        assert_eq!(jobs.len(), 1);
    }
}
