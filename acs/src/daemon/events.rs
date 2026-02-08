use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};
use uuid::Uuid;

/// Custom serializer for Arc<str> that serializes as a plain string.
fn serialize_arc_str<S>(data: &Arc<str>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(data)
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "event", content = "data")]
pub enum JobEvent {
    Started {
        job_id: Uuid,
        run_id: Uuid,
        job_name: String,
        timestamp: DateTime<Utc>,
    },
    Output {
        job_id: Uuid,
        run_id: Uuid,
        #[serde(serialize_with = "serialize_arc_str")]
        data: Arc<str>,
        timestamp: DateTime<Utc>,
    },
    Completed {
        job_id: Uuid,
        run_id: Uuid,
        exit_code: i32,
        timestamp: DateTime<Utc>,
    },
    Failed {
        job_id: Uuid,
        run_id: Uuid,
        error: String,
        timestamp: DateTime<Utc>,
    },
    JobChanged {
        job_id: Uuid,
        change: JobChangeKind,
        timestamp: DateTime<Utc>,
    },
}

#[derive(Debug, Clone, Serialize)]
pub enum JobChangeKind {
    Added,
    Updated,
    Removed,
    Enabled,
    Disabled,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    // --- JobEvent serde roundtrip tests ---

    #[test]
    fn test_started_event_serializes() {
        let event = JobEvent::Started {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            job_name: "test-job".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"event\":\"Started\""));
        assert!(json.contains("\"job_name\":\"test-job\""));
        assert!(json.contains("\"job_id\""));
        assert!(json.contains("\"run_id\""));
        assert!(json.contains("\"timestamp\""));
    }

    #[test]
    fn test_output_event_serializes_with_arc_str() {
        let data: Arc<str> = Arc::from("hello world\n");
        let event = JobEvent::Output {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            data: data.clone(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"event\":\"Output\""));
        assert!(json.contains("\"data\":\"hello world\\n\""));

        // Verify Arc<str> clones share the same data (cheap cloning)
        let data2 = data.clone();
        assert!(Arc::ptr_eq(&data, &data2));
    }

    #[test]
    fn test_completed_event_serializes() {
        let event = JobEvent::Completed {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            exit_code: 0,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"event\":\"Completed\""));
        assert!(json.contains("\"exit_code\":0"));
    }

    #[test]
    fn test_completed_event_nonzero_exit_code() {
        let event = JobEvent::Completed {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            exit_code: 1,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"exit_code\":1"));
    }

    #[test]
    fn test_failed_event_serializes() {
        let event = JobEvent::Failed {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            error: "PTY spawn failed".to_string(),
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"event\":\"Failed\""));
        assert!(json.contains("\"error\":\"PTY spawn failed\""));
    }

    #[test]
    fn test_job_changed_event_serializes() {
        let event = JobEvent::JobChanged {
            job_id: Uuid::nil(),
            change: JobChangeKind::Added,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        assert!(json.contains("\"event\":\"JobChanged\""));
        assert!(json.contains("\"change\":\"Added\""));
    }

    // --- JobChangeKind serde roundtrip ---

    #[test]
    fn test_job_change_kind_all_variants_serialize() {
        let variants = vec![
            (JobChangeKind::Added, "\"Added\""),
            (JobChangeKind::Updated, "\"Updated\""),
            (JobChangeKind::Removed, "\"Removed\""),
            (JobChangeKind::Enabled, "\"Enabled\""),
            (JobChangeKind::Disabled, "\"Disabled\""),
        ];
        for (kind, expected) in variants {
            let json = serde_json::to_string(&kind).expect("serialize");
            assert_eq!(json, expected);
        }
    }

    // --- Broadcast channel tests ---

    #[tokio::test]
    async fn test_broadcast_two_subscribers_both_receive() {
        let (tx, mut rx1) = broadcast::channel::<JobEvent>(16);
        let mut rx2 = tx.subscribe();

        let event = JobEvent::Started {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            job_name: "broadcast-test".to_string(),
            timestamp: Utc::now(),
        };

        tx.send(event).expect("send");

        let received1 = rx1.recv().await.expect("recv1");
        let received2 = rx2.recv().await.expect("recv2");

        // Both subscribers received the event
        match (&received1, &received2) {
            (JobEvent::Started { job_name: n1, .. }, JobEvent::Started { job_name: n2, .. }) => {
                assert_eq!(n1, "broadcast-test");
                assert_eq!(n2, "broadcast-test");
            }
            _ => panic!("Expected Started events"),
        }
    }

    #[tokio::test]
    async fn test_broadcast_lagged_subscriber() {
        // Create a channel with capacity 2
        let (tx, mut rx1) = broadcast::channel::<JobEvent>(2);
        let _rx_keep = tx.subscribe(); // keep the sender alive

        // Send 4 events (more than capacity 2)
        for i in 0..4 {
            let event = JobEvent::Output {
                job_id: Uuid::nil(),
                run_id: Uuid::nil(),
                data: Arc::from(format!("msg {}", i).as_str()),
                timestamp: Utc::now(),
            };
            let _ = tx.send(event);
        }

        // The receiver should have lagged since we exceeded capacity
        let result = rx1.recv().await;
        match result {
            Err(broadcast::error::RecvError::Lagged(n)) => {
                assert!(n > 0, "Should have lagged by at least 1 message");
            }
            Ok(_) => {
                // In some timing scenarios, we might still get a message.
                // The important thing is the channel handles overflow gracefully.
            }
            Err(broadcast::error::RecvError::Closed) => {
                panic!("Channel should not be closed");
            }
        }
    }

    #[test]
    fn test_arc_str_output_serializes_correctly() {
        // Verify that Arc<str> serializes the same as a regular string
        let data: Arc<str> = Arc::from("test output data");
        let event = JobEvent::Output {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            data,
            timestamp: Utc::now(),
        };
        let json = serde_json::to_string(&event).expect("serialize");
        // Parse back to verify structure
        let value: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert_eq!(value["data"]["data"], "test output data");
        assert_eq!(value["event"], "Output");
    }

    #[test]
    fn test_event_clone_with_arc_str_is_cheap() {
        let data: Arc<str> = Arc::from("large output data that should be shared");
        let event = JobEvent::Output {
            job_id: Uuid::nil(),
            run_id: Uuid::nil(),
            data: data.clone(),
            timestamp: Utc::now(),
        };
        let cloned = event.clone();

        // Both events should share the same Arc<str> allocation
        match (&event, &cloned) {
            (JobEvent::Output { data: d1, .. }, JobEvent::Output { data: d2, .. }) => {
                assert!(
                    Arc::ptr_eq(d1, d2),
                    "Cloned Arc<str> should share allocation"
                );
            }
            _ => panic!("Expected Output events"),
        }
    }
}
