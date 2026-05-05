use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, Mutex};
use uuid::Uuid;

use crate::daemon::events::WorkflowEvent;
use crate::workflow::step::LogSink;

// ─── CurrentStep ─────────────────────────────────────────────────────────────

/// Tracks the step that is currently executing so that chunk events carry the
/// right `step_index` and `step_id`.
struct CurrentStep {
    step_index: usize,
    step_id: String,
}

// ─── EventEmittingLogSink ─────────────────────────────────────────────────────

/// A [`LogSink`] wrapper that:
///
/// 1. Forwards every method call to an inner `Arc<dyn LogSink>` (delegate pattern).
/// 2. Emits a [`WorkflowEvent::StepOutput`] on every `write_chunk` call so that
///    SSE subscribers receive real-time stdout/stderr chunks.
///
/// The `step_index` / `step_id` for each chunk event are captured via
/// [`set_current_step`], which the executor calls before invoking each step's
/// `execute()`.  If `set_current_step` has never been called, `write_chunk`
/// still forwards to the inner sink but does **not** emit an event.
pub struct EventEmittingLogSink {
    inner: Arc<dyn LogSink>,
    event_tx: broadcast::Sender<WorkflowEvent>,
    run_id: Uuid,
    workflow_id: Uuid,
    current_step: Mutex<Option<CurrentStep>>,
}

impl EventEmittingLogSink {
    /// Create a new wrapper around `inner`.
    pub fn new(
        inner: Arc<dyn LogSink>,
        event_tx: broadcast::Sender<WorkflowEvent>,
        run_id: Uuid,
        workflow_id: Uuid,
    ) -> Self {
        Self {
            inner,
            event_tx,
            run_id,
            workflow_id,
            current_step: Mutex::new(None),
        }
    }
}

#[async_trait]
impl LogSink for EventEmittingLogSink {
    /// Update the tracked current step and forward to the inner sink.
    async fn set_current_step(&self, step_index: usize, step_id: &str) -> std::io::Result<()> {
        *self.current_step.lock().await = Some(CurrentStep {
            step_index,
            step_id: step_id.to_string(),
        });
        self.inner.set_current_step(step_index, step_id).await
    }

    async fn write_step_start(
        &self,
        step_id: &str,
        started_at: DateTime<Utc>,
    ) -> std::io::Result<u64> {
        self.inner.write_step_start(step_id, started_at).await
    }

    /// Write the chunk to the inner sink and emit a `StepOutput` event.
    ///
    /// The inner write is always performed.  If `set_current_step` has not
    /// been called yet the event is silently skipped.  Send errors (no
    /// receivers) are also silently ignored.
    async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
        // Emit event best-effort; don't hold the lock across the inner write.
        {
            let guard = self.current_step.lock().await;
            if let Some(cs) = guard.as_ref() {
                let chunk_str: Arc<str> = String::from_utf8_lossy(data).into_owned().into();
                let _ = self.event_tx.send(WorkflowEvent::StepOutput {
                    run_id: self.run_id,
                    workflow_id: self.workflow_id,
                    step_index: cs.step_index,
                    step_id: cs.step_id.clone(),
                    data: chunk_str,
                    timestamp: Utc::now(),
                });
            }
        }
        self.inner.write_chunk(data).await
    }

    async fn write_step_end(
        &self,
        step_id: &str,
        exit_code: Option<i32>,
        finished_at: DateTime<Utc>,
    ) -> std::io::Result<u64> {
        self.inner
            .write_step_end(step_id, exit_code, finished_at)
            .await
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::sync::Mutex as StdMutex;
    use tokio::sync::broadcast;
    use uuid::Uuid;

    // ── Mock inner LogSink ────────────────────────────────────────────────────

    /// Records every `write_chunk` call for assertions.
    #[derive(Clone, Default)]
    struct MockInnerSink {
        chunks: Arc<StdMutex<Vec<Vec<u8>>>>,
        set_step_calls: Arc<StdMutex<Vec<(usize, String)>>>,
    }

    #[async_trait]
    impl LogSink for MockInnerSink {
        async fn set_current_step(&self, step_index: usize, step_id: &str) -> std::io::Result<()> {
            self.set_step_calls
                .lock()
                .unwrap()
                .push((step_index, step_id.to_string()));
            Ok(())
        }

        async fn write_step_start(
            &self,
            _step_id: &str,
            _started_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            Ok(0)
        }

        async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
            self.chunks.lock().unwrap().push(data.to_vec());
            Ok(())
        }

        async fn write_step_end(
            &self,
            _step_id: &str,
            _exit_code: Option<i32>,
            _finished_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            Ok(0)
        }
    }

    fn make_wrapper(
        capacity: usize,
    ) -> (
        EventEmittingLogSink,
        MockInnerSink,
        broadcast::Receiver<WorkflowEvent>,
        Uuid,
        Uuid,
    ) {
        let inner = MockInnerSink::default();
        let (tx, rx) = broadcast::channel::<WorkflowEvent>(capacity);
        let run_id = Uuid::now_v7();
        let workflow_id = Uuid::now_v7();
        let wrapper = EventEmittingLogSink::new(
            Arc::new(inner.clone()) as Arc<dyn LogSink>,
            tx,
            run_id,
            workflow_id,
        );
        (wrapper, inner, rx, run_id, workflow_id)
    }

    // ── Test 1: set_current_step + write_chunk emits correct step fields ──────

    #[tokio::test]
    async fn test_set_current_step_propagates_to_chunk_events() {
        let (wrapper, _inner, mut rx, _run_id, _workflow_id) = make_wrapper(16);

        wrapper.set_current_step(3, "fetch").await.unwrap();
        wrapper.write_chunk(b"hello").await.unwrap();

        let event = rx.recv().await.expect("should receive event");
        match event {
            WorkflowEvent::StepOutput {
                step_index,
                step_id,
                data,
                ..
            } => {
                assert_eq!(step_index, 3);
                assert_eq!(step_id, "fetch");
                assert_eq!(data.as_ref(), "hello");
            }
            other => panic!("Expected StepOutput, got {:?}", other),
        }
    }

    // ── Test 2: chunk event carries correct run_id and workflow_id ────────────

    #[tokio::test]
    async fn test_chunk_event_includes_run_id_and_workflow_id() {
        let (wrapper, _inner, mut rx, expected_run_id, expected_workflow_id) = make_wrapper(16);

        wrapper.set_current_step(0, "step-a").await.unwrap();
        wrapper.write_chunk(b"data").await.unwrap();

        let event = rx.recv().await.expect("should receive event");
        match event {
            WorkflowEvent::StepOutput {
                run_id,
                workflow_id,
                ..
            } => {
                assert_eq!(run_id, expected_run_id);
                assert_eq!(workflow_id, expected_workflow_id);
            }
            other => panic!("Expected StepOutput, got {:?}", other),
        }
    }

    // ── Test 3: no subscribers → no panic ─────────────────────────────────────

    #[tokio::test]
    async fn test_chunk_event_with_no_subscribers_doesnt_panic() {
        let inner = MockInnerSink::default();
        let (tx, rx) = broadcast::channel::<WorkflowEvent>(16);
        // Drop the only receiver so there are no subscribers.
        drop(rx);

        let wrapper = EventEmittingLogSink::new(
            Arc::new(inner) as Arc<dyn LogSink>,
            tx,
            Uuid::now_v7(),
            Uuid::now_v7(),
        );

        wrapper.set_current_step(0, "s").await.unwrap();
        // Should not panic even though there are no receivers.
        wrapper.write_chunk(b"chunk").await.unwrap();
    }

    // ── Test 4: write_chunk before set_current_step emits nothing ─────────────

    #[tokio::test]
    async fn test_chunk_event_without_set_current_step_skips() {
        let (wrapper, _inner, mut rx, _run_id, _workflow_id) = make_wrapper(16);

        // Do NOT call set_current_step.
        wrapper.write_chunk(b"orphan chunk").await.unwrap();

        // No event should be available.
        match rx.try_recv() {
            Err(broadcast::error::TryRecvError::Empty) => {
                // Correct: nothing was emitted.
            }
            Ok(e) => panic!("Expected no event, got {:?}", e),
            Err(e) => panic!("Unexpected recv error: {:?}", e),
        }
    }

    // ── Test 5: inner sink still receives chunks ───────────────────────────────

    #[tokio::test]
    async fn test_inner_logsink_still_receives_chunks() {
        let (wrapper, inner, _rx, _, _) = make_wrapper(16);

        wrapper.set_current_step(1, "run").await.unwrap();
        wrapper.write_chunk(b"line1\n").await.unwrap();
        wrapper.write_chunk(b"line2\n").await.unwrap();

        let chunks = inner.chunks.lock().unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(chunks[0], b"line1\n");
        assert_eq!(chunks[1], b"line2\n");
    }

    // ── Test 6: set_current_step overwrites previous ──────────────────────────

    #[tokio::test]
    async fn test_set_current_step_overwrites_previous() {
        let (wrapper, _inner, mut rx, _, _) = make_wrapper(32);

        // First step
        wrapper.set_current_step(1, "alpha").await.unwrap();
        wrapper.write_chunk(b"first").await.unwrap();

        let e1 = rx.recv().await.expect("first event");
        match e1 {
            WorkflowEvent::StepOutput {
                step_index,
                step_id,
                data,
                ..
            } => {
                assert_eq!(step_index, 1);
                assert_eq!(step_id, "alpha");
                assert_eq!(data.as_ref(), "first");
            }
            other => panic!("Expected StepOutput, got {:?}", other),
        }

        // Switch to second step
        wrapper.set_current_step(2, "beta").await.unwrap();
        wrapper.write_chunk(b"second").await.unwrap();

        let e2 = rx.recv().await.expect("second event");
        match e2 {
            WorkflowEvent::StepOutput {
                step_index,
                step_id,
                data,
                ..
            } => {
                assert_eq!(step_index, 2);
                assert_eq!(step_id, "beta");
                assert_eq!(data.as_ref(), "second");
            }
            other => panic!("Expected StepOutput, got {:?}", other),
        }
    }

    // ── Test 7: set_current_step is forwarded to inner sink ───────────────────

    #[tokio::test]
    async fn test_set_current_step_forwarded_to_inner() {
        let (wrapper, inner, _rx, _, _) = make_wrapper(16);

        wrapper.set_current_step(5, "my-step").await.unwrap();

        let calls = inner.set_step_calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0], (5usize, "my-step".to_string()));
    }

    // ── Test 8: write_step_start / write_step_end delegate to inner ──────────

    #[tokio::test]
    async fn test_write_step_start_and_end_delegate_to_inner() {
        let (wrapper, _inner, _rx, _, _) = make_wrapper(16);
        let t = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();

        // These should not panic and should return the inner's result.
        let start_offset = wrapper.write_step_start("s1", t).await.unwrap();
        assert_eq!(start_offset, 0);

        let end_offset = wrapper.write_step_end("s1", Some(0), t).await.unwrap();
        assert_eq!(end_offset, 0);
    }
}
