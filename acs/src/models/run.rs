use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RunStatus {
    Running,
    Completed,
    CompletedWithWarnings,
    Failed,
    Killed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JobRun {
    pub run_id: Uuid,
    pub job_id: Uuid,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
    pub exit_code: Option<i32>,
    pub log_size_bytes: u64,
    pub error: Option<String>,
    /// Trigger-time parameter overrides used for this run, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trigger_params: Option<crate::models::TriggerParams>,
    /// Total cost from the Claude CLI result event (in USD)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost_usd: Option<f64>,
    /// CLI-reported execution duration (in milliseconds)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// Number of turns from the result event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub num_turns: Option<u32>,
    /// Primary model from the system event
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Full usage data blob from the result event (token counts per model)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_job_run() -> JobRun {
        JobRun {
            run_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            status: RunStatus::Completed,
            exit_code: Some(0),
            log_size_bytes: 1024,
            error: None,
            trigger_params: None,
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        }
    }

    #[test]
    fn test_job_run_serde_roundtrip() {
        let run = make_job_run();
        let json = serde_json::to_string(&run).expect("serialize");
        let deserialized: JobRun = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(run, deserialized);
    }

    #[test]
    fn test_run_status_running_serde() {
        let status = RunStatus::Running;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"Running\"");
        let deserialized: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_run_status_completed_serde() {
        let status = RunStatus::Completed;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"Completed\"");
        let deserialized: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_run_status_completed_with_warnings_serde() {
        let status = RunStatus::CompletedWithWarnings;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"CompletedWithWarnings\"");
        let deserialized: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_run_status_failed_serde() {
        let status = RunStatus::Failed;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"Failed\"");
        let deserialized: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_run_status_killed_serde() {
        let status = RunStatus::Killed;
        let json = serde_json::to_string(&status).expect("serialize");
        assert_eq!(json, "\"Killed\"");
        let deserialized: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(status, deserialized);
    }

    #[test]
    fn test_job_run_with_error() {
        let run = JobRun {
            run_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            started_at: Utc::now(),
            finished_at: Some(Utc::now()),
            status: RunStatus::Failed,
            exit_code: None,
            log_size_bytes: 0,
            error: Some("PTY spawn failed".to_string()),
            trigger_params: None,
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let deserialized: JobRun = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(run, deserialized);
        assert_eq!(deserialized.error, Some("PTY spawn failed".to_string()));
    }

    #[test]
    fn test_job_run_running_no_finish() {
        let run = JobRun {
            run_id: Uuid::now_v7(),
            job_id: Uuid::now_v7(),
            started_at: Utc::now(),
            finished_at: None,
            status: RunStatus::Running,
            exit_code: None,
            log_size_bytes: 0,
            error: None,
            trigger_params: None,
            total_cost_usd: None,
            duration_ms: None,
            num_turns: None,
            model: None,
            usage: None,
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let deserialized: JobRun = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.finished_at.is_none());
        assert!(deserialized.exit_code.is_none());
    }

    #[test]
    fn test_job_run_backward_compat_without_cost_fields() {
        // Test that JSON lacking cost fields deserializes with all cost fields as None
        let old_json = r#"{
            "run_id": "01234567-8901-2345-6789-012345678901",
            "job_id": "98765432-1098-7654-3210-987654321098",
            "started_at": "2025-01-01T00:00:00Z",
            "finished_at": "2025-01-01T00:01:00Z",
            "status": "Completed",
            "exit_code": 0,
            "log_size_bytes": 1024,
            "error": null
        }"#;

        let deserialized: JobRun = serde_json::from_str(old_json).expect("deserialize");
        assert!(deserialized.total_cost_usd.is_none());
        assert!(deserialized.duration_ms.is_none());
        assert!(deserialized.num_turns.is_none());
        assert!(deserialized.model.is_none());
        assert!(deserialized.usage.is_none());
    }
}
