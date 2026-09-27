//! Mid-run persistence of per-step progress.
//!
//! The executor calls a [`StepPersister`] at every step boundary (when a step
//! starts and when it finishes) so that the stored run record reflects the
//! steps executed so far while the run is still in progress. This lets
//! clients that refetch a running run see a `Running` row for the active step
//! and final rows for the steps that already finished.
//!
//! Mid-run persistence is best-effort: a failed write is logged and ignored,
//! and never affects step execution. `finalize_run` remains the authoritative
//! write of the complete run record once the run finishes.

use std::sync::Arc;

use async_trait::async_trait;
use uuid::Uuid;

use crate::models::workflow::StepRun;
use crate::storage::workflow_runs::WorkflowRunStore;

/// Sink for the executor's in-progress `StepRun` list.
///
/// Implementations must not fail the run: errors are handled internally
/// (typically logged). The executor always passes the full `steps` list for
/// the run so far, in execution order.
#[async_trait]
pub trait StepPersister: Send + Sync {
    /// Persist the current `steps` list for `run_id`. Best-effort.
    async fn persist_steps(&self, run_id: Uuid, steps: &[StepRun]);
}

/// [`StepPersister`] backed by a [`WorkflowRunStore`].
///
/// Calls [`WorkflowRunStore::update_run_steps`], which replaces only the
/// `steps` field of the run record and never touches run-level fields such as
/// `status`. Errors are logged at `warn` level and swallowed; `finalize_run`
/// writes the authoritative record when the run completes.
pub struct RunStoreStepPersister {
    store: Arc<dyn WorkflowRunStore>,
}

impl RunStoreStepPersister {
    /// Create a persister that writes through `store`.
    pub fn new(store: Arc<dyn WorkflowRunStore>) -> Self {
        Self { store }
    }
}

#[async_trait]
impl StepPersister for RunStoreStepPersister {
    async fn persist_steps(&self, run_id: Uuid, steps: &[StepRun]) {
        if let Err(e) = self.store.update_run_steps(run_id, steps).await {
            tracing::warn!(
                run_id = %run_id,
                error = %e,
                "mid-run step persistence failed (non-fatal; finalize_run is authoritative)"
            );
        }
    }
}
