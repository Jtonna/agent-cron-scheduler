use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::workflow::{NewWorkflow, RunStatus, Workflow, WorkflowUpdate};

// ─── WorkflowStore trait ──────────────────────────────────────────────────────

#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn list_workflows(&self) -> Result<Vec<Workflow>>;
    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>>;
    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow>;
    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow>;
    async fn delete_workflow(&self, id: Uuid) -> Result<()>;

    /// Record the terminal outcome of a run on its parent workflow.
    ///
    /// Updates `last_run_id`, `last_run_status`, `last_run_at`, and bumps
    /// `updated_at`. Does **not** bump `version` — that field is reserved for
    /// definition-level changes (steps, schedule, env, etc.). Returns
    /// `AcsError::NotFound` if `workflow_id` does not match any row.
    async fn record_run_outcome(
        &self,
        workflow_id: Uuid,
        run_id: Uuid,
        status: RunStatus,
        finished_at: DateTime<Utc>,
    ) -> Result<()>;
}
