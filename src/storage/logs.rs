use std::path::PathBuf;

use anyhow::{Context, Result};
use async_trait::async_trait;
use uuid::Uuid;

use crate::models::JobRun;
use crate::storage::LogStore;

pub struct FsLogStore {
    logs_dir: PathBuf,
}

impl FsLogStore {
    /// Create a new FsLogStore rooted at data_dir/logs/.
    pub async fn new(data_dir: PathBuf) -> Result<Self> {
        let logs_dir = data_dir.join("logs");
        tokio::fs::create_dir_all(&logs_dir)
            .await
            .context("Failed to create logs directory")?;
        Ok(Self { logs_dir })
    }

    /// Get the directory for a specific job's logs.
    fn job_dir(&self, job_id: Uuid) -> PathBuf {
        self.logs_dir.join(job_id.to_string())
    }

    /// Get the path to a run's metadata file.
    fn meta_path(&self, job_id: Uuid, run_id: Uuid) -> PathBuf {
        self.job_dir(job_id).join(format!("{}.meta.json", run_id))
    }

    /// Get the path to a run's log file.
    fn log_path(&self, job_id: Uuid, run_id: Uuid) -> PathBuf {
        self.job_dir(job_id).join(format!("{}.log", run_id))
    }
}

#[async_trait]
impl LogStore for FsLogStore {
    async fn create_run(&self, run: &JobRun) -> Result<()> {
        let job_dir = self.job_dir(run.job_id);
        tokio::fs::create_dir_all(&job_dir)
            .await
            .context("Failed to create job log directory")?;

        let meta_path = self.meta_path(run.job_id, run.run_id);
        let json = serde_json::to_string_pretty(run).context("Failed to serialize run metadata")?;
        tokio::fs::write(&meta_path, json.as_bytes())
            .await
            .context("Failed to write run metadata")?;

        Ok(())
    }

    async fn update_run(&self, run: &JobRun) -> Result<()> {
        let meta_path = self.meta_path(run.job_id, run.run_id);
        let json = serde_json::to_string_pretty(run).context("Failed to serialize run metadata")?;
        tokio::fs::write(&meta_path, json.as_bytes())
            .await
            .context("Failed to write run metadata")?;
        Ok(())
    }

    async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> Result<()> {
        let job_dir = self.job_dir(job_id);
        tokio::fs::create_dir_all(&job_dir)
            .await
            .context("Failed to create job log directory")?;

        let log_path = self.log_path(job_id, run_id);

        use tokio::io::AsyncWriteExt;
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .await
            .context("Failed to open log file for appending")?;

        file.write_all(data)
            .await
            .context("Failed to append to log file")?;

        file.flush().await.context("Failed to flush log file")?;

        Ok(())
    }

    async fn read_log(&self, job_id: Uuid, run_id: Uuid, tail: Option<usize>) -> Result<String> {
        let log_path = self.log_path(job_id, run_id);

        if !log_path.exists() {
            return Ok(String::new());
        }

        let content = tokio::fs::read_to_string(&log_path)
            .await
            .context("Failed to read log file")?;

        match tail {
            Some(n) => {
                let lines: Vec<&str> = content.lines().collect();
                let start = if lines.len() > n { lines.len() - n } else { 0 };
                let tail_lines = &lines[start..];
                Ok(tail_lines.join("\n"))
            }
            None => Ok(content),
        }
    }

    async fn list_runs(
        &self,
        job_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobRun>, usize)> {
        let job_dir = self.job_dir(job_id);

        if !job_dir.exists() {
            return Ok((Vec::new(), 0));
        }

        let mut runs = Vec::new();
        let mut entries = tokio::fs::read_dir(&job_dir)
            .await
            .context("Failed to read job log directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".meta.json") {
                    let content = tokio::fs::read_to_string(&path)
                        .await
                        .context("Failed to read run metadata")?;
                    match serde_json::from_str::<JobRun>(&content) {
                        Ok(run) => runs.push(run),
                        Err(e) => {
                            tracing::warn!("Skipping malformed meta file {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        // Sort by started_at descending
        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));

        let total = runs.len();

        // Apply offset and limit
        let paginated: Vec<JobRun> = runs.into_iter().skip(offset).take(limit).collect();

        Ok((paginated, total))
    }

    async fn cleanup(&self, job_id: Uuid, max_files: usize) -> Result<()> {
        let job_dir = self.job_dir(job_id);

        if !job_dir.exists() {
            return Ok(());
        }

        // Read all meta files to get runs sorted by started_at
        let mut runs = Vec::new();
        let mut entries = tokio::fs::read_dir(&job_dir)
            .await
            .context("Failed to read job log directory")?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.ends_with(".meta.json") {
                    let content = tokio::fs::read_to_string(&path)
                        .await
                        .context("Failed to read run metadata")?;
                    match serde_json::from_str::<JobRun>(&content) {
                        Ok(run) => runs.push(run),
                        Err(e) => {
                            tracing::warn!("Skipping malformed meta file {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        if runs.len() <= max_files {
            return Ok(());
        }

        // Sort by started_at ascending (oldest first)
        runs.sort_by(|a, b| a.started_at.cmp(&b.started_at));

        let to_remove = runs.len() - max_files;
        for run in runs.iter().take(to_remove) {
            let meta_path = self.meta_path(job_id, run.run_id);
            let log_path = self.log_path(job_id, run.run_id);

            if meta_path.exists() {
                tokio::fs::remove_file(&meta_path)
                    .await
                    .context("Failed to remove old meta file")?;
            }
            if log_path.exists() {
                tokio::fs::remove_file(&log_path)
                    .await
                    .context("Failed to remove old log file")?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RunStatus;
    use chrono::Utc;
    use tempfile::TempDir;

    fn make_job_run(job_id: Uuid) -> JobRun {
        JobRun {
            run_id: Uuid::now_v7(),
            job_id,
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            exit_code: None,
            log_size_bytes: 0,
            error: None,
        }
    }

    async fn setup_store() -> (FsLogStore, TempDir, Uuid) {
        let tmp_dir = TempDir::new().expect("create temp dir");
        let store = FsLogStore::new(tmp_dir.path().to_path_buf())
            .await
            .expect("create store");
        let job_id = Uuid::now_v7();
        (store, tmp_dir, job_id)
    }

    #[tokio::test]
    async fn test_create_run_writes_meta_json() {
        let (store, tmp, job_id) = setup_store().await;
        let run = make_job_run(job_id);
        store.create_run(&run).await.expect("create run");

        let meta_path = tmp
            .path()
            .join("logs")
            .join(job_id.to_string())
            .join(format!("{}.meta.json", run.run_id));
        assert!(meta_path.exists(), "meta.json file should exist");

        let content = tokio::fs::read_to_string(&meta_path).await.expect("read");
        let loaded: JobRun = serde_json::from_str(&content).expect("parse");
        assert_eq!(loaded.run_id, run.run_id);
        assert_eq!(loaded.job_id, run.job_id);
    }

    #[tokio::test]
    async fn test_update_run() {
        let (store, _tmp, job_id) = setup_store().await;
        let mut run = make_job_run(job_id);
        store.create_run(&run).await.expect("create run");

        run.status = RunStatus::Completed;
        run.exit_code = Some(0);
        run.finished_at = Some(Utc::now());
        run.log_size_bytes = 512;
        store.update_run(&run).await.expect("update run");

        // Verify the update persisted
        let (runs, _) = store.list_runs(job_id, 10, 0).await.expect("list");
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, RunStatus::Completed);
        assert_eq!(runs[0].exit_code, Some(0));
    }

    #[tokio::test]
    async fn test_append_and_read_log_roundtrip() {
        let (store, _tmp, job_id) = setup_store().await;
        let run = make_job_run(job_id);
        store.create_run(&run).await.expect("create run");

        store
            .append_log(job_id, run.run_id, b"line 1\n")
            .await
            .expect("append");
        store
            .append_log(job_id, run.run_id, b"line 2\n")
            .await
            .expect("append");
        store
            .append_log(job_id, run.run_id, b"line 3\n")
            .await
            .expect("append");

        let content = store
            .read_log(job_id, run.run_id, None)
            .await
            .expect("read");
        assert_eq!(content, "line 1\nline 2\nline 3\n");
    }

    #[tokio::test]
    async fn test_read_log_nonexistent_returns_empty() {
        let (store, _tmp, job_id) = setup_store().await;
        let content = store
            .read_log(job_id, Uuid::now_v7(), None)
            .await
            .expect("read");
        assert!(content.is_empty());
    }

    #[tokio::test]
    async fn test_read_log_with_tail() {
        let (store, _tmp, job_id) = setup_store().await;
        let run = make_job_run(job_id);
        store.create_run(&run).await.expect("create run");

        let data = "line 1\nline 2\nline 3\nline 4\nline 5\n";
        store
            .append_log(job_id, run.run_id, data.as_bytes())
            .await
            .expect("append");

        let content = store
            .read_log(job_id, run.run_id, Some(2))
            .await
            .expect("read");
        assert_eq!(content, "line 4\nline 5");
    }

    #[tokio::test]
    async fn test_read_log_tail_larger_than_file() {
        let (store, _tmp, job_id) = setup_store().await;
        let run = make_job_run(job_id);
        store.create_run(&run).await.expect("create run");

        store
            .append_log(job_id, run.run_id, b"only line\n")
            .await
            .expect("append");

        let content = store
            .read_log(job_id, run.run_id, Some(100))
            .await
            .expect("read");
        assert_eq!(content, "only line");
    }

    #[tokio::test]
    async fn test_list_runs_empty() {
        let (store, _tmp, job_id) = setup_store().await;
        let (runs, total) = store.list_runs(job_id, 10, 0).await.expect("list");
        assert!(runs.is_empty());
        assert_eq!(total, 0);
    }

    #[tokio::test]
    async fn test_list_runs_multiple() {
        let (store, _tmp, job_id) = setup_store().await;

        for _ in 0..5 {
            let run = make_job_run(job_id);
            store.create_run(&run).await.expect("create run");
            // Small delay so started_at values differ
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let (runs, total) = store.list_runs(job_id, 10, 0).await.expect("list");
        assert_eq!(runs.len(), 5);
        assert_eq!(total, 5);

        // Verify sorted by started_at desc
        for i in 1..runs.len() {
            assert!(runs[i - 1].started_at >= runs[i].started_at);
        }
    }

    #[tokio::test]
    async fn test_list_runs_pagination_limit() {
        let (store, _tmp, job_id) = setup_store().await;

        for _ in 0..5 {
            let run = make_job_run(job_id);
            store.create_run(&run).await.expect("create run");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let (runs, total) = store.list_runs(job_id, 2, 0).await.expect("list");
        assert_eq!(runs.len(), 2);
        assert_eq!(total, 5);
    }

    #[tokio::test]
    async fn test_list_runs_pagination_offset() {
        let (store, _tmp, job_id) = setup_store().await;

        for _ in 0..5 {
            let run = make_job_run(job_id);
            store.create_run(&run).await.expect("create run");
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        let (runs, total) = store.list_runs(job_id, 10, 3).await.expect("list");
        assert_eq!(runs.len(), 2);
        assert_eq!(total, 5);
    }

    #[tokio::test]
    async fn test_list_runs_returns_total_count() {
        let (store, _tmp, job_id) = setup_store().await;

        for _ in 0..7 {
            let run = make_job_run(job_id);
            store.create_run(&run).await.expect("create run");
        }

        let (_, total) = store.list_runs(job_id, 3, 0).await.expect("list");
        assert_eq!(total, 7);
    }

    #[tokio::test]
    async fn test_cleanup_removes_oldest_beyond_max() {
        let (store, _tmp, job_id) = setup_store().await;

        let mut run_ids = Vec::new();
        for _ in 0..5 {
            let run = make_job_run(job_id);
            run_ids.push(run.run_id);
            store.create_run(&run).await.expect("create run");
            store
                .append_log(job_id, run.run_id, b"some data\n")
                .await
                .expect("append");
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }

        // Keep only 2 newest
        store.cleanup(job_id, 2).await.expect("cleanup");

        let (runs, total) = store.list_runs(job_id, 10, 0).await.expect("list");
        assert_eq!(runs.len(), 2);
        assert_eq!(total, 2);
    }

    #[tokio::test]
    async fn test_cleanup_no_op_within_limit() {
        let (store, _tmp, job_id) = setup_store().await;

        for _ in 0..3 {
            let run = make_job_run(job_id);
            store.create_run(&run).await.expect("create run");
        }

        store.cleanup(job_id, 10).await.expect("cleanup");

        let (runs, total) = store.list_runs(job_id, 10, 0).await.expect("list");
        assert_eq!(runs.len(), 3);
        assert_eq!(total, 3);
    }

    #[tokio::test]
    async fn test_cleanup_removes_log_files_too() {
        let (store, tmp, job_id) = setup_store().await;

        let run = make_job_run(job_id);
        store.create_run(&run).await.expect("create run");
        store
            .append_log(job_id, run.run_id, b"data\n")
            .await
            .expect("append");

        // Verify log file exists
        let log_path = tmp
            .path()
            .join("logs")
            .join(job_id.to_string())
            .join(format!("{}.log", run.run_id));
        assert!(log_path.exists());

        // Add one more newer run
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let run2 = make_job_run(job_id);
        store.create_run(&run2).await.expect("create run");

        // Cleanup to keep only 1
        store.cleanup(job_id, 1).await.expect("cleanup");

        // Old log file should be gone
        assert!(!log_path.exists(), "Old log file should have been removed");
    }

    #[tokio::test]
    async fn test_cleanup_nonexistent_job_dir() {
        let (store, _tmp, _) = setup_store().await;
        // Cleanup on a job that has no log directory should succeed silently
        let result = store.cleanup(Uuid::now_v7(), 5).await;
        assert!(result.is_ok());
    }
}
