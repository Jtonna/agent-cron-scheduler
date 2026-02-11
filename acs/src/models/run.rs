use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum RunStatus {
    Running,
    Completed,
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
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let deserialized: JobRun = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.finished_at.is_none());
        assert!(deserialized.exit_code.is_none());
    }
}
