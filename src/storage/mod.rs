pub mod jobs;
pub mod logs;

use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::models::{Job, JobRun, JobUpdate, NewJob};

#[async_trait]
pub trait JobStore: Send + Sync {
    async fn list_jobs(&self) -> Result<Vec<Job>>;
    async fn get_job(&self, id: Uuid) -> Result<Option<Job>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Job>>;
    async fn create_job(&self, new: NewJob) -> Result<Job>;
    async fn update_job(&self, id: Uuid, update: JobUpdate) -> Result<Job>;
    async fn delete_job(&self, id: Uuid) -> Result<()>;
}

#[async_trait]
pub trait LogStore: Send + Sync {
    async fn create_run(&self, run: &JobRun) -> Result<()>;
    async fn update_run(&self, run: &JobRun) -> Result<()>;
    async fn append_log(&self, job_id: Uuid, run_id: Uuid, data: &[u8]) -> Result<()>;
    async fn read_log(&self, job_id: Uuid, run_id: Uuid, tail: Option<usize>) -> Result<String>;
    async fn list_runs(
        &self,
        job_id: Uuid,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<JobRun>, usize)>;
    async fn cleanup(&self, job_id: Uuid, max_files: usize) -> Result<()>;
}
