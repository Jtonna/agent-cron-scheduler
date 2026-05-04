use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use tokio::sync::{broadcast, watch};
use uuid::Uuid;

/// Channel types used to signal a run should be killed.
///
/// `watch` is used because:
///   - Multiple receivers can clone from a single sender (one per step).
///   - The latest value is always immediately available via `borrow()`.
///   - Dropping all senders automatically signals shutdown to receivers.
pub type KillSender = watch::Sender<bool>;
pub type KillReceiver = watch::Receiver<bool>;

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
    /// Accumulated outputs keyed by step id, in insertion order.
    ///
    /// `IndexMap` preserves the order in which steps are inserted, so
    /// `.values().last()` always returns the immediately-prior step's output.
    /// This makes `pass_stdin` deterministic: it pipes the last-inserted
    /// (immediately preceding) step's stdout to the current step's stdin.
    pub steps: IndexMap<String, StepOutput>,
    pub log_sink: Arc<dyn LogSink>,
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    /// Optional broadcast sender for `WorkflowEvent` SSE streaming.
    /// `None` in tests and contexts that don't require event broadcasting.
    pub event_tx: Option<broadcast::Sender<crate::daemon::events::WorkflowEvent>>,
    /// Optional kill signal receiver. When `Some`, steps poll this for a `true`
    /// value and abort with `StepError::Killed` when signalled.
    /// `None` in tests and contexts that don't wire up the kill registry.
    pub kill_rx: Option<KillReceiver>,
    /// Optional step id whose stdin should receive the trigger's `input` value.
    ///
    /// Set from `TriggerParams.target_step` at the start of `run_workflow`.
    /// When the executor reaches a `ShellStep` or `ScriptStep` whose id matches
    /// this value, the trigger's `input` is serialized and written to that
    /// step's stdin. This OVERRIDES `pass_stdin` for the matching step.
    /// Step kinds that do not accept stdin (e.g. `MatchStep`, `SetVarStep`,
    /// `HttpStep`, `AgentStep`) ignore this value silently. A non-matching id
    /// is also ignored silently — the workflow runs without stdin routing.
    pub target_step: Option<String>,
}

/// A future that resolves when the kill receiver is signalled with `true`.
///
/// Returns immediately if the current value is already `true` (i.e., kill was
/// sent before this step started). Never resolves if `kill_rx` is `None` or if
/// the sender is dropped without sending `true` (sender-dropped means the run
/// finished normally before a kill arrived).
pub async fn wait_for_kill(kill_rx: Option<KillReceiver>) {
    match kill_rx {
        None => std::future::pending().await,
        Some(mut rx) => loop {
            if *rx.borrow() {
                return;
            }
            match rx.changed().await {
                Ok(()) => {
                    if *rx.borrow() {
                        return;
                    }
                    // Value changed back to false — shouldn't happen in normal
                    // usage but handle defensively by waiting for next change.
                }
                Err(_) => {
                    // Sender dropped without setting true — run finished
                    // normally; never resolve.
                    std::future::pending::<()>().await;
                    return;
                }
            }
        },
    }
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
    /// Inform the sink which step is now executing.
    ///
    /// Called by the executor before invoking each step's `execute()`.
    /// The default implementation is a no-op so that existing `LogSink`
    /// implementations (e.g. `FileLogSink`) do not require changes.
    async fn set_current_step(&self, step_index: usize, step_id: &str) -> std::io::Result<()> {
        let _ = (step_index, step_id);
        Ok(())
    }

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
