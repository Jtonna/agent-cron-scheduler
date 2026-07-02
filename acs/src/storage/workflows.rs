use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::workflow::{NewWorkflow, RunStatus, Workflow, WorkflowUpdate};

// ─── WorkflowStore trait ──────────────────────────────────────────────────────

#[async_trait]
pub trait WorkflowStore: Send + Sync {
    /// List all **live** (non-deleted) workflows. Soft-deleted workflows
    /// are excluded so they disappear from the UI, scheduler, and
    /// name resolution.
    async fn list_workflows(&self) -> Result<Vec<Workflow>>;

    /// List every workflow row, **including** soft-deleted ones. Used only by
    /// the cost endpoints, which must keep surfacing the persisted cost/token
    /// history of deleted workflows (flagged via `Workflow::deleted`).
    async fn list_workflows_including_deleted(&self) -> Result<Vec<Workflow>>;

    /// Fetch a single **live** workflow. Returns `None` for a missing row *or*
    /// a soft-deleted one — deleted workflows behave as "not found".
    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>>;

    /// Fetch a single workflow row **including** soft-deleted ones. Used by the
    /// per-id cost endpoint so cost history for a deleted workflow is still
    /// returned (200 with `workflow_deleted: true`).
    async fn get_workflow_including_deleted(&self, id: Uuid) -> Result<Option<Workflow>>;

    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>>;
    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow>;
    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow>;

    /// Soft-delete a workflow: flag the row `deleted = 1` and release
    /// its unique name so the name can be reused, while keeping the row and all
    /// of its `workflow_runs` so cost history survives. Returns
    /// `AcsError::NotFound` if the id does not match a live workflow.
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

    /// Toggle the `is_favorited` flag on a workflow and return the updated
    /// record. Bumps `updated_at` but does NOT bump `version` — favorites are
    /// a UI-only preference, not a definition change. Returns
    /// `AcsError::NotFound` if `id` doesn't exist.
    async fn set_favorited(&self, id: Uuid, is_favorited: bool) -> Result<Workflow>;
}
