//! Run-completion plumbing shared between the scheduler and the
//! `/api/workflows/{id}/trigger` route.
//!
//! Both paths run the same two-step dance after `run_workflow` returns:
//!   1. Persist the final `WorkflowRun` row.
//!   2. Stamp the parent `Workflow` with the terminal status / timestamp so
//!      that `list_workflows` / `get_workflow` reflect it in `last_run_*`.
//!
//! Centralising this here keeps the `tracing::error!` log format consistent
//! across call sites (the format that's important for log consumers).

use std::sync::Arc;

use chrono::Utc;

use crate::models::workflow::WorkflowRun;
use crate::storage::workflow_runs::WorkflowRunStore;
use crate::storage::workflows::WorkflowStore;

/// Persist a terminal `WorkflowRun` and propagate its outcome to the parent
/// workflow. Errors are logged but not returned — callers cannot meaningfully
/// recover from a write failure in the post-run path.
pub async fn finalize_run(
    run_store: &Arc<dyn WorkflowRunStore>,
    workflow_store: &Arc<dyn WorkflowStore>,
    run: &WorkflowRun,
) {
    if let Err(e) = run_store.update_run(run).await {
        tracing::error!(
            workflow_id = %run.workflow_id,
            "Failed to persist final run {}: {}",
            run.run_id, e
        );
    }

    let finished_at = run.finished_at.unwrap_or_else(Utc::now);
    if let Err(e) = workflow_store
        .record_run_outcome(run.workflow_id, run.run_id, run.status, finished_at)
        .await
    {
        tracing::error!(
            workflow_id = %run.workflow_id,
            "Failed to record run outcome on parent workflow for run {}: {}",
            run.run_id, e
        );
    }
}
