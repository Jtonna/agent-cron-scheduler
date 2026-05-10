use async_trait::async_trait;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::WorkflowRun;

// ─── WorkflowRunStore trait ───────────────────────────────────────────────────

#[async_trait]
pub trait WorkflowRunStore: Send + Sync {
    /// Create a new run record (status=Running, started_at=now).
    async fn create_run(&self, run: WorkflowRun) -> Result<(), AcsError>;

    /// Update an existing run record.
    async fn update_run(&self, run: &WorkflowRun) -> Result<(), AcsError>;

    /// Get a single run by id.
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

    /// Delete a run record. Best-effort.
    async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError>;
}
