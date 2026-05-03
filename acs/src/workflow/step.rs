use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

/// Cost / usage data extracted from an agent step's output.
/// (Phase 4 will populate this from streaming NDJSON parsers.)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct CostFragment {
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub num_turns: Option<u32>,
    pub model: Option<String>,
    pub usage: Option<serde_json::Value>,
}

/// Output of a single step's execution. Returned by `Step::execute`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StepOutput {
    pub exit_code: Option<i32>,
    /// Structured if parseable, else raw string wrapped in Value::String.
    pub stdout: Option<serde_json::Value>,
    /// Explicit named exports.
    pub exports: HashMap<String, serde_json::Value>,
    pub cost: Option<CostFragment>,
}

/// Errors a step can return that prevent it from producing a `StepOutput`.
///
/// Note: step-level non-zero exits are NOT errors — they're returned as
/// `StepOutput { exit_code: Some(non_zero), ... }`.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("spawn error: {0}")]
    Spawn(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timeout after {0} seconds")]
    Timeout(u64),
    #[error("template substitution failed: {0}")]
    Template(String),
    #[error("kill requested")]
    Killed,
    #[error("internal: {0}")]
    Internal(String),
}

/// Mutable execution context passed to each step's `execute()`.
pub struct StepContext {
    pub workflow_id: Uuid,
    pub workflow_version: u32,
    pub run_id: Uuid,
    pub step_index: usize,
    pub input: serde_json::Value,
    /// Accumulated outputs keyed by step id.
    pub steps: HashMap<String, StepOutput>,
    pub log_sink: Arc<dyn LogSink>,
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    /// Optional broadcast sender for `WorkflowEvent` SSE streaming.
    /// `None` in tests and contexts that don't require event broadcasting.
    pub event_tx: Option<broadcast::Sender<crate::daemon::events::WorkflowEvent>>,
}

#[async_trait]
pub trait Step: Send + Sync {
    fn kind(&self) -> &'static str;
    async fn execute(&self, ctx: &mut StepContext) -> Result<StepOutput, StepError>;
}

/// Abstraction over the run's combined log file.
///
/// Step impls call this to write output and step-boundary markers.
/// Returns byte offsets used to populate
/// `StepRun.log_byte_offset_start` / `_end`.
#[async_trait]
pub trait LogSink: Send + Sync {
    /// Write the START marker for a step.
    /// Returns the byte offset of the BEGINNING of the marker line.
    async fn write_step_start(&self, step_id: &str, started_at: DateTime<Utc>)
        -> std::io::Result<u64>;

    /// Append output bytes (stdout+stderr interleaved) to the log.
    async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()>;

    /// Write the END marker for a step.
    /// Returns the byte offset just AFTER the marker line ended.
    async fn write_step_end(
        &self,
        step_id: &str,
        exit_code: Option<i32>,
        finished_at: DateTime<Utc>,
    ) -> std::io::Result<u64>;
}
