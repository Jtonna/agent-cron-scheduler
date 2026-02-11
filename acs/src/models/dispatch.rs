use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Job;

/// Per-trigger parameter overrides. All fields optional.
/// Deserialized from the trigger endpoint request body.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct TriggerParams {
    /// Additional arguments appended to the shell command for this run.
    pub args: Option<String>,
    /// Environment variables merged on top of job.env_vars for this run only.
    pub env: Option<HashMap<String, String>>,
    /// String written to the process's stdin after launch.
    pub input: Option<String>,
}

/// Wrapper sent through the dispatch channel from trigger/scheduler to the executor.
#[derive(Debug, Clone)]
pub struct DispatchRequest {
    /// The job to execute.
    pub job: Job,
    /// Pre-generated run ID (v7 UUID) for this execution.
    pub run_id: Uuid,
    /// Optional trigger-time parameter overrides.
    pub trigger_params: Option<TriggerParams>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trigger_params_deserialize_all_fields() {
        let json = r#"{"args":"--flag value","env":{"KEY":"VAL"},"input":"hello"}"#;
        let params: TriggerParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.args.as_deref(), Some("--flag value"));
        assert_eq!(params.env.as_ref().unwrap().get("KEY").unwrap(), "VAL");
        assert_eq!(params.input.as_deref(), Some("hello"));
    }

    #[test]
    fn test_trigger_params_deserialize_partial() {
        let json = r#"{"args":"--flag"}"#;
        let params: TriggerParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.args.as_deref(), Some("--flag"));
        assert!(params.env.is_none());
        assert!(params.input.is_none());
    }

    #[test]
    fn test_trigger_params_deserialize_empty_object() {
        let json = r#"{}"#;
        let params: TriggerParams = serde_json::from_str(json).unwrap();
        assert!(params.args.is_none());
        assert!(params.env.is_none());
        assert!(params.input.is_none());
    }

    #[test]
    fn test_trigger_params_default() {
        let params = TriggerParams::default();
        assert!(params.args.is_none());
        assert!(params.env.is_none());
        assert!(params.input.is_none());
    }

    #[test]
    fn test_dispatch_request_construction() {
        let job = Job {
            id: Uuid::now_v7(),
            name: "test".to_string(),
            schedule: "* * * * *".to_string(),
            execution: crate::models::ExecutionType::ShellCommand("echo hi".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        };
        let run_id = Uuid::now_v7();
        let req = DispatchRequest {
            job: job.clone(),
            run_id,
            trigger_params: Some(TriggerParams {
                args: Some("--extra".to_string()),
                env: None,
                input: None,
            }),
        };
        assert_eq!(req.run_id, run_id);
        assert_eq!(req.job.name, "test");
        assert!(req.trigger_params.is_some());
    }
}
