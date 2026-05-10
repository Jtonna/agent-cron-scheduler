use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::models::workflow::{NewWorkflow, Workflow, WorkflowUpdate};

// ─── WorkflowStore trait ──────────────────────────────────────────────────────

#[async_trait]
pub trait WorkflowStore: Send + Sync {
    async fn list_workflows(&self) -> Result<Vec<Workflow>>;
    async fn get_workflow(&self, id: Uuid) -> Result<Option<Workflow>>;
    async fn find_by_name(&self, name: &str) -> Result<Option<Workflow>>;
    async fn create_workflow(&self, new: NewWorkflow) -> Result<Workflow>;
    async fn update_workflow(&self, id: Uuid, update: WorkflowUpdate) -> Result<Workflow>;
    async fn delete_workflow(&self, id: Uuid) -> Result<()>;
}
