//! Server-Sent Events handlers.
//!
//! Only the workflow-event SSE handler remains after Phase 6 cleanup.
//! The old JobEvent SSE handler was removed along with the job infrastructure.

use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Query, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::Deserialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use uuid::Uuid;

use super::AppState;
use crate::daemon::events::WorkflowEvent;

/// Guard that logs at debug level when the SSE stream is dropped (client disconnects).
struct SseDropGuard;

impl Drop for SseDropGuard {
    fn drop(&mut self) {
        tracing::debug!("SSE client disconnected");
    }
}

#[derive(Debug, Deserialize, Default)]
pub struct WorkflowSseParams {
    pub run_id: Option<String>,
    pub workflow_id: Option<String>,
}

/// GET /api/events/workflows — SSE stream of WorkflowEvent.
///
/// Supports optional `run_id` and `workflow_id` query filters.
pub async fn workflow_events_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<WorkflowSseParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("Workflow SSE client connected");

    let rx = state.workflow_event_tx.subscribe();

    let filter_run_id = params.run_id.and_then(|s| Uuid::parse_str(&s).ok());
    let filter_workflow_id = params.workflow_id.and_then(|s| Uuid::parse_str(&s).ok());

    let _drop_guard = SseDropGuard;

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let _ = &_drop_guard;
        match result {
            Ok(event) => {
                // Optional run_id filter
                if let Some(fr) = filter_run_id {
                    let event_run_id = match &event {
                        WorkflowEvent::RunStarted { run_id, .. } => Some(*run_id),
                        WorkflowEvent::StepStarted { run_id, .. } => Some(*run_id),
                        WorkflowEvent::StepOutput { run_id, .. } => Some(*run_id),
                        WorkflowEvent::StepCompleted { run_id, .. } => Some(*run_id),
                        WorkflowEvent::RunCompleted { run_id, .. } => Some(*run_id),
                        WorkflowEvent::RunFailed { run_id, .. } => Some(*run_id),
                        WorkflowEvent::WorkflowChanged { .. } => None,
                    };
                    if event_run_id != Some(fr) {
                        return None;
                    }
                }

                // Optional workflow_id filter
                if let Some(fw) = filter_workflow_id {
                    let event_wf_id = match &event {
                        WorkflowEvent::RunStarted { workflow_id, .. } => Some(*workflow_id),
                        WorkflowEvent::StepStarted { workflow_id, .. } => Some(*workflow_id),
                        WorkflowEvent::StepOutput { workflow_id, .. } => Some(*workflow_id),
                        WorkflowEvent::StepCompleted { workflow_id, .. } => Some(*workflow_id),
                        WorkflowEvent::RunCompleted { workflow_id, .. } => Some(*workflow_id),
                        WorkflowEvent::RunFailed { workflow_id, .. } => Some(*workflow_id),
                        WorkflowEvent::WorkflowChanged { workflow_id, .. } => Some(*workflow_id),
                    };
                    if event_wf_id != Some(fw) {
                        return None;
                    }
                }

                // Determine event type name
                let event_type = match &event {
                    WorkflowEvent::RunStarted { .. } => "run_started",
                    WorkflowEvent::StepStarted { .. } => "step_started",
                    WorkflowEvent::StepOutput { .. } => "step_output",
                    WorkflowEvent::StepCompleted { .. } => "step_completed",
                    WorkflowEvent::RunCompleted { .. } => "run_completed",
                    WorkflowEvent::RunFailed { .. } => "run_failed",
                    WorkflowEvent::WorkflowChanged { .. } => "workflow_changed",
                };

                match serde_json::to_string(&event) {
                    Ok(data) => Some(Ok(Event::default().event(event_type).data(data))),
                    Err(_) => None,
                }
            }
            Err(_) => {
                // Lagged — send a comment to inform the client and continue
                Some(Ok(
                    Event::default().comment("lagged: some workflow events were missed")
                ))
            }
        }
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}
