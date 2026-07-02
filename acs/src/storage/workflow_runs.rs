use async_trait::async_trait;
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::{CostSummary, DailyBucket, WorkflowRun};

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

    /// List runs across all workflows, latest-first. Pagination via limit + offset.
    /// limit=0 means "no limit" (return all).
    async fn list_recent_runs(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<WorkflowRun>, AcsError>;

    /// Total count of runs across all workflows.
    async fn count_all_runs(&self) -> Result<usize, AcsError>;

    /// Delete a run record. Best-effort. Cost-ledger rows are untouched.
    async fn delete_run(&self, run_id: Uuid) -> Result<(), AcsError>;

    /// Delete every non-Running run for the workflow in one transaction and
    /// return the deleted run ids. Running runs are skipped. Cost-ledger rows
    /// are untouched.
    async fn purge_runs(&self, workflow_id: Uuid) -> Result<Vec<Uuid>, AcsError>;

    /// Distinct `(workflow_id, workflow_name)` pairs present in the durable
    /// cost ledger, using the name recorded on each workflow's most recent
    /// terminal run. Includes workflows that have since been deleted.
    async fn list_ledger_workflows(&self) -> Result<Vec<(Uuid, String)>, AcsError>;

    /// Compute cost aggregates for a workflow over the last 30 days and last
    /// year, using calendar-day boundaries in `display_tz`.
    async fn cost_summary_for(
        &self,
        workflow_id: Uuid,
        display_tz: &Tz,
    ) -> Result<CostSummary, AcsError>;

    /// Return per-day cost buckets for the given time window.
    ///
    /// `workflow_id = None` aggregates across all workflows (system-wide).
    /// Results are in ascending date order; days with no terminal runs are omitted.
    /// Date grouping uses `display_tz` for calendar-day boundaries.
    async fn daily_buckets_for(
        &self,
        workflow_id: Option<Uuid>,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
        display_tz: &Tz,
    ) -> Result<Vec<DailyBucket>, AcsError>;
}
