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
use crate::daemon::events::JobEvent;

#[derive(Debug, Deserialize, Default)]
pub struct SseParams {
    pub job_id: Option<String>,
    pub run_id: Option<String>,
}

/// Guard that logs at debug level when the SSE stream is dropped (client disconnects).
struct SseDropGuard;

impl Drop for SseDropGuard {
    fn drop(&mut self) {
        tracing::debug!("SSE client disconnected");
    }
}

pub async fn sse_handler(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SseParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    tracing::info!("SSE client connected");

    let rx = state.event_tx.subscribe();

    // Parse filter UUIDs
    let filter_job_id = params.job_id.and_then(|s| Uuid::parse_str(&s).ok());
    let filter_run_id = params.run_id.and_then(|s| Uuid::parse_str(&s).ok());

    // The drop guard is moved into the closure so it lives as long as the stream.
    // When the client disconnects and the stream is dropped, the guard logs the disconnect.
    let _drop_guard = SseDropGuard;

    let stream = BroadcastStream::new(rx).filter_map(move |result| {
        let _ = &_drop_guard;
        match result {
            Ok(event) => {
                // Apply filters
                if let Some(fj) = filter_job_id {
                    let event_job_id = match &event {
                        JobEvent::Started { job_id, .. } => Some(*job_id),
                        JobEvent::Output { job_id, .. } => Some(*job_id),
                        JobEvent::Completed { job_id, .. } => Some(*job_id),
                        JobEvent::Failed { job_id, .. } => Some(*job_id),
                        JobEvent::JobChanged { job_id, .. } => Some(*job_id),
                    };
                    if event_job_id != Some(fj) {
                        return None;
                    }
                }

                if let Some(fr) = filter_run_id {
                    let event_run_id = match &event {
                        JobEvent::Started { run_id, .. } => Some(*run_id),
                        JobEvent::Output { run_id, .. } => Some(*run_id),
                        JobEvent::Completed { run_id, .. } => Some(*run_id),
                        JobEvent::Failed { run_id, .. } => Some(*run_id),
                        JobEvent::JobChanged { .. } => None,
                    };
                    if event_run_id != Some(fr) {
                        return None;
                    }
                }

                // Determine event type name
                let event_type = match &event {
                    JobEvent::Started { .. } => "started",
                    JobEvent::Output { .. } => "output",
                    JobEvent::Completed { .. } => "completed",
                    JobEvent::Failed { .. } => "failed",
                    JobEvent::JobChanged { .. } => "job_changed",
                };

                match serde_json::to_string(&event) {
                    Ok(data) => Some(Ok(Event::default().event(event_type).data(data))),
                    Err(_) => None,
                }
            }
            Err(_) => {
                // Lagged -- send a comment to inform the client and continue
                Some(Ok(
                    Event::default().comment("lagged: some events were missed")
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
