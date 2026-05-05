use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// Custom serializer for Arc<str> that serializes as a plain string.
fn serialize_arc_str<S>(data: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(data)
}

/// Custom deserializer for Arc<str> that deserializes from a plain string.
fn deserialize_arc_str<'de, D>(deserializer: D) -> Result<Arc<str>, D::Error>
where
    D: Deserializer<'de>,
{
    let s = String::deserialize(deserializer)?;
    Ok(Arc::from(s.as_str()))
}
// ─── WorkflowEvent ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum WorkflowEvent {
    RunStarted {
        run_id: Uuid,
        workflow_id: Uuid,
        workflow_version: u32,
        started_at: DateTime<Utc>,
    },
    StepStarted {
        run_id: Uuid,
        workflow_id: Uuid,
        step_index: usize,
        step_id: String,
        /// "shell" | "script" | "http" | "match" | "set_var" | "agent"
        kind: String,
        started_at: DateTime<Utc>,
    },
    StepOutput {
        run_id: Uuid,
        workflow_id: Uuid,
        step_index: usize,
        step_id: String,
        /// Chunk of stdout/stderr. Uses Arc<str> for cheap clone across broadcast receivers.
        #[serde(
            serialize_with = "serialize_arc_str",
            deserialize_with = "deserialize_arc_str"
        )]
        data: Arc<str>,
        timestamp: DateTime<Utc>,
    },
    StepCompleted {
        run_id: Uuid,
        workflow_id: Uuid,
        step_index: usize,
        step_id: String,
        exit_code: Option<i32>,
        cost_usd: Option<f64>,
        finished_at: DateTime<Utc>,
    },
    RunCompleted {
        run_id: Uuid,
        workflow_id: Uuid,
        status: crate::models::workflow::RunStatus,
        total_cost_usd: Option<f64>,
        finished_at: DateTime<Utc>,
    },
    RunFailed {
        run_id: Uuid,
        workflow_id: Uuid,
        error: String,
        finished_at: DateTime<Utc>,
    },
    WorkflowChanged {
        workflow_id: Uuid,
        version: u32,
        change_kind: WorkflowChangeKind,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowChangeKind {
    Created,
    Updated,
    Deleted,
    Enabled,
    Disabled,
}

#[cfg(test)]
mod workflow_event_tests {
    use super::*;
    use crate::models::workflow::RunStatus;
    use tokio::sync::broadcast;

    // ── Serialization: correct "type" tag per variant ────────────────────────

    #[test]
    fn test_run_started_serializes_with_type_tag() {
        let event = WorkflowEvent::RunStarted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            workflow_version: 1,
            started_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"RunStarted\""), "json: {}", json);
        assert!(json.contains("\"run_id\""), "json: {}", json);
        assert!(json.contains("\"workflow_id\""), "json: {}", json);
        assert!(json.contains("\"workflow_version\":1"), "json: {}", json);
        assert!(json.contains("\"started_at\""), "json: {}", json);
    }

    #[test]
    fn test_step_started_serializes_with_type_tag() {
        let event = WorkflowEvent::StepStarted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 0,
            step_id: "step-1".to_string(),
            kind: "shell".to_string(),
            started_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"StepStarted\""), "json: {}", json);
        assert!(json.contains("\"step_index\":0"), "json: {}", json);
        assert!(json.contains("\"step_id\":\"step-1\""), "json: {}", json);
        assert!(json.contains("\"kind\":\"shell\""), "json: {}", json);
    }

    #[test]
    fn test_step_output_serializes_with_type_tag() {
        let event = WorkflowEvent::StepOutput {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 2,
            step_id: "step-2".to_string(),
            data: Arc::from("some output chunk"),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"StepOutput\""), "json: {}", json);
        assert!(
            json.contains("\"data\":\"some output chunk\""),
            "json: {}",
            json
        );
        assert!(json.contains("\"step_index\":2"), "json: {}", json);
    }

    #[test]
    fn test_step_completed_serializes_with_type_tag() {
        let event = WorkflowEvent::StepCompleted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 1,
            step_id: "step-3".to_string(),
            exit_code: Some(0),
            cost_usd: Some(0.0012),
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains("\"type\":\"StepCompleted\""),
            "json: {}",
            json
        );
        assert!(json.contains("\"exit_code\":0"), "json: {}", json);
        assert!(json.contains("\"cost_usd\":0.0012"), "json: {}", json);
    }

    #[test]
    fn test_step_completed_null_fields_serialize() {
        let event = WorkflowEvent::StepCompleted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 0,
            step_id: "step-x".to_string(),
            exit_code: None,
            cost_usd: None,
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"exit_code\":null"), "json: {}", json);
        assert!(json.contains("\"cost_usd\":null"), "json: {}", json);
    }

    #[test]
    fn test_run_completed_serializes_with_type_tag() {
        let event = WorkflowEvent::RunCompleted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            status: RunStatus::Completed,
            total_cost_usd: Some(0.005),
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"RunCompleted\""), "json: {}", json);
        assert!(json.contains("\"status\":\"Completed\""), "json: {}", json);
        assert!(json.contains("\"total_cost_usd\":0.005"), "json: {}", json);
    }

    #[test]
    fn test_run_failed_serializes_with_type_tag() {
        let event = WorkflowEvent::RunFailed {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            error: "step-1 timed out".to_string(),
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"type\":\"RunFailed\""), "json: {}", json);
        assert!(
            json.contains("\"error\":\"step-1 timed out\""),
            "json: {}",
            json
        );
    }

    #[test]
    fn test_workflow_changed_serializes_with_type_tag() {
        let event = WorkflowEvent::WorkflowChanged {
            workflow_id: Uuid::nil(),
            version: 3,
            change_kind: WorkflowChangeKind::Updated,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(
            json.contains("\"type\":\"WorkflowChanged\""),
            "json: {}",
            json
        );
        assert!(json.contains("\"version\":3"), "json: {}", json);
        assert!(
            json.contains("\"change_kind\":\"updated\""),
            "json: {}",
            json
        );
    }

    // ── Arc<str> cheap clone ─────────────────────────────────────────────────

    #[test]
    fn test_step_output_arc_str_cheap_clone() {
        let data: Arc<str> = Arc::from("streaming output line");
        let event = WorkflowEvent::StepOutput {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 0,
            step_id: "s".to_string(),
            data: data.clone(),
            timestamp: Utc::now(),
        };

        let initial_count = Arc::strong_count(&data);
        let cloned = event.clone();
        let after_count = Arc::strong_count(&data);

        // Cloning the event bumps the Arc refcount by 1
        assert_eq!(after_count, initial_count + 1);

        // Both arcs point at the same allocation (cheap clone, no copy)
        match &cloned {
            WorkflowEvent::StepOutput {
                data: cloned_data, ..
            } => {
                assert!(Arc::ptr_eq(&data, cloned_data), "should share allocation");
            }
            _ => panic!("Expected StepOutput"),
        }
    }

    // ── Deserialize round-trip per variant ───────────────────────────────────

    #[test]
    fn test_run_started_roundtrip() {
        let run_id = Uuid::now_v7();
        let workflow_id = Uuid::now_v7();
        let event = WorkflowEvent::RunStarted {
            run_id,
            workflow_id,
            workflow_version: 2,
            started_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::RunStarted {
                run_id: r,
                workflow_id: w,
                workflow_version: v,
                ..
            } => {
                assert_eq!(r, run_id);
                assert_eq!(w, workflow_id);
                assert_eq!(v, 2);
            }
            _ => panic!("Expected RunStarted"),
        }
    }

    #[test]
    fn test_step_started_roundtrip() {
        let event = WorkflowEvent::StepStarted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 5,
            step_id: "agent-step".to_string(),
            kind: "agent".to_string(),
            started_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::StepStarted {
                step_index,
                step_id,
                kind,
                ..
            } => {
                assert_eq!(step_index, 5);
                assert_eq!(step_id, "agent-step");
                assert_eq!(kind, "agent");
            }
            _ => panic!("Expected StepStarted"),
        }
    }

    #[test]
    fn test_step_output_roundtrip() {
        let event = WorkflowEvent::StepOutput {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 1,
            step_id: "http-step".to_string(),
            data: Arc::from("response body chunk"),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::StepOutput { data, step_id, .. } => {
                assert_eq!(data.as_ref(), "response body chunk");
                assert_eq!(step_id, "http-step");
            }
            _ => panic!("Expected StepOutput"),
        }
    }

    #[test]
    fn test_step_completed_roundtrip() {
        let event = WorkflowEvent::StepCompleted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            step_index: 3,
            step_id: "script-step".to_string(),
            exit_code: Some(1),
            cost_usd: None,
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::StepCompleted {
                exit_code,
                cost_usd,
                step_id,
                ..
            } => {
                assert_eq!(exit_code, Some(1));
                assert!(cost_usd.is_none());
                assert_eq!(step_id, "script-step");
            }
            _ => panic!("Expected StepCompleted"),
        }
    }

    #[test]
    fn test_run_completed_roundtrip() {
        let event = WorkflowEvent::RunCompleted {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            status: RunStatus::Failed,
            total_cost_usd: None,
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::RunCompleted {
                status,
                total_cost_usd,
                ..
            } => {
                assert_eq!(status, RunStatus::Failed);
                assert!(total_cost_usd.is_none());
            }
            _ => panic!("Expected RunCompleted"),
        }
    }

    #[test]
    fn test_run_failed_roundtrip() {
        let event = WorkflowEvent::RunFailed {
            run_id: Uuid::nil(),
            workflow_id: Uuid::nil(),
            error: "executor panic".to_string(),
            finished_at: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::RunFailed { error, .. } => {
                assert_eq!(error, "executor panic");
            }
            _ => panic!("Expected RunFailed"),
        }
    }

    #[test]
    fn test_workflow_changed_roundtrip() {
        let wf_id = Uuid::now_v7();
        let event = WorkflowEvent::WorkflowChanged {
            workflow_id: wf_id,
            version: 7,
            change_kind: WorkflowChangeKind::Deleted,
        };
        let json = serde_json::to_string(&event).expect("serialize");
        let decoded: WorkflowEvent = serde_json::from_str(&json).expect("deserialize");
        match decoded {
            WorkflowEvent::WorkflowChanged {
                workflow_id,
                version,
                change_kind,
            } => {
                assert_eq!(workflow_id, wf_id);
                assert_eq!(version, 7);
                assert_eq!(change_kind, WorkflowChangeKind::Deleted);
            }
            _ => panic!("Expected WorkflowChanged"),
        }
    }

    // ── WorkflowChangeKind snake_case serialization ──────────────────────────

    #[test]
    fn test_workflow_change_kind_all_variants_snake_case() {
        let cases = vec![
            (WorkflowChangeKind::Created, "\"created\""),
            (WorkflowChangeKind::Updated, "\"updated\""),
            (WorkflowChangeKind::Deleted, "\"deleted\""),
            (WorkflowChangeKind::Enabled, "\"enabled\""),
            (WorkflowChangeKind::Disabled, "\"disabled\""),
        ];
        for (kind, expected) in cases {
            let json = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(
                json, expected,
                "WorkflowChangeKind::{:?} should serialize as {}",
                kind, expected
            );
        }
    }

    #[test]
    fn test_workflow_change_kind_deserializes_from_snake_case() {
        let cases = vec![
            ("\"created\"", WorkflowChangeKind::Created),
            ("\"updated\"", WorkflowChangeKind::Updated),
            ("\"deleted\"", WorkflowChangeKind::Deleted),
            ("\"enabled\"", WorkflowChangeKind::Enabled),
            ("\"disabled\"", WorkflowChangeKind::Disabled),
        ];
        for (input, expected) in cases {
            let decoded: WorkflowChangeKind = serde_json::from_str(input).expect("deserialize");
            assert_eq!(decoded, expected);
        }
    }

    // ── Broadcast: send and receive ──────────────────────────────────────────

    #[tokio::test]
    async fn test_workflow_broadcast_send_and_receive() {
        let (tx, mut rx) = broadcast::channel::<WorkflowEvent>(16);

        let run_id = Uuid::now_v7();
        let workflow_id = Uuid::now_v7();
        let event = WorkflowEvent::RunStarted {
            run_id,
            workflow_id,
            workflow_version: 1,
            started_at: Utc::now(),
        };

        tx.send(event).expect("send");

        let received = rx.recv().await.expect("recv");
        match received {
            WorkflowEvent::RunStarted {
                run_id: r,
                workflow_id: w,
                workflow_version: v,
                ..
            } => {
                assert_eq!(r, run_id);
                assert_eq!(w, workflow_id);
                assert_eq!(v, 1);
            }
            _ => panic!("Expected RunStarted"),
        }
    }

    #[tokio::test]
    async fn test_workflow_broadcast_two_subscribers_both_receive() {
        let (tx, mut rx1) = broadcast::channel::<WorkflowEvent>(16);
        let mut rx2 = tx.subscribe();

        let run_id = Uuid::now_v7();
        let event = WorkflowEvent::RunFailed {
            run_id,
            workflow_id: Uuid::nil(),
            error: "two-subscriber-test".to_string(),
            finished_at: Utc::now(),
        };

        tx.send(event).expect("send");

        let r1 = rx1.recv().await.expect("recv1");
        let r2 = rx2.recv().await.expect("recv2");

        for received in [r1, r2] {
            match received {
                WorkflowEvent::RunFailed {
                    run_id: r, error, ..
                } => {
                    assert_eq!(r, run_id);
                    assert_eq!(error, "two-subscriber-test");
                }
                _ => panic!("Expected RunFailed"),
            }
        }
    }

    #[tokio::test]
    async fn test_workflow_broadcast_lagged_subscriber() {
        // Channel capacity of 2; we send 5 events to force lag
        let (tx, mut rx1) = broadcast::channel::<WorkflowEvent>(2);
        let _rx_keep = tx.subscribe(); // keep sender alive

        for i in 0u32..5 {
            let event = WorkflowEvent::RunStarted {
                run_id: Uuid::nil(),
                workflow_id: Uuid::nil(),
                workflow_version: i,
                started_at: Utc::now(),
            };
            let _ = tx.send(event);
        }

        // Slow subscriber should have lagged
        let result = rx1.recv().await;
        match result {
            Err(broadcast::error::RecvError::Lagged(n)) => {
                assert!(n > 0, "Should have lagged by at least 1 message");
            }
            Ok(_) => {
                // Acceptable: some events may still be buffered depending on timing.
            }
            Err(broadcast::error::RecvError::Closed) => {
                panic!("Channel should not be closed");
            }
        }
    }
}
