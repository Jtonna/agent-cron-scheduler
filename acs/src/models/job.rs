use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AcsError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum ExecutionType {
    ShellCommand(String),
    ScriptFile(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: Uuid,
    pub name: String,
    pub schedule: String,
    pub execution: ExecutionType,
    pub enabled: bool,
    pub timezone: Option<String>,
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    #[serde(default)]
    pub timeout_secs: u64,
    #[serde(default)]
    pub log_environment: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_exit_code: Option<i32>,
    #[serde(skip_deserializing, default)]
    pub next_run_at: Option<DateTime<Utc>>,
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.schedule == other.schedule
            && self.execution == other.execution
            && self.enabled == other.enabled
            && self.timezone == other.timezone
            && self.working_dir == other.working_dir
            && self.env_vars == other.env_vars
            && self.timeout_secs == other.timeout_secs
            && self.log_environment == other.log_environment
            && self.created_at == other.created_at
            && self.updated_at == other.updated_at
            && self.last_run_at == other.last_run_at
            && self.last_exit_code == other.last_exit_code
        // next_run_at is skipped (computed, not persisted)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewJob {
    pub name: String,
    pub schedule: String,
    pub execution: ExecutionType,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub timezone: Option<String>,
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    #[serde(default)]
    pub timeout_secs: u64,
    #[serde(default)]
    pub log_environment: bool,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct JobUpdate {
    pub name: Option<String>,
    pub schedule: Option<String>,
    pub execution: Option<ExecutionType>,
    pub enabled: Option<bool>,
    pub timezone: Option<String>,
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub timeout_secs: Option<u64>,
    pub log_environment: Option<bool>,
    /// Internal metadata: set to Some(Some(ts)) to update, Some(None) to clear.
    /// Skipped during JSON deserialization from API clients (not user-editable).
    #[serde(skip)]
    pub last_run_at: Option<Option<DateTime<Utc>>>,
    /// Internal metadata: set to Some(Some(code)) to update, Some(None) to clear.
    #[serde(skip)]
    pub last_exit_code: Option<Option<i32>>,
}

/// Validate a NewJob before creation.
pub fn validate_new_job(job: &NewJob) -> Result<(), AcsError> {
    // Name cannot be empty
    if job.name.trim().is_empty() {
        return Err(AcsError::Validation("Job name cannot be empty".to_string()));
    }

    // Name cannot be a valid UUID
    if Uuid::parse_str(&job.name).is_ok() {
        return Err(AcsError::Validation(
            "Job name cannot be a valid UUID".to_string(),
        ));
    }

    // Validate cron expression with croner
    validate_cron(&job.schedule)?;

    // Validate timezone if provided
    if let Some(ref tz) = job.timezone {
        validate_timezone(tz)?;
    }

    Ok(())
}

/// Validate a JobUpdate before applying.
pub fn validate_job_update(update: &JobUpdate) -> Result<(), AcsError> {
    if let Some(ref name) = update.name {
        if name.trim().is_empty() {
            return Err(AcsError::Validation("Job name cannot be empty".to_string()));
        }
        if Uuid::parse_str(name).is_ok() {
            return Err(AcsError::Validation(
                "Job name cannot be a valid UUID".to_string(),
            ));
        }
    }

    if let Some(ref schedule) = update.schedule {
        validate_cron(schedule)?;
    }

    if let Some(ref tz) = update.timezone {
        validate_timezone(tz)?;
    }

    Ok(())
}

fn validate_cron(expr: &str) -> Result<(), AcsError> {
    use croner::Cron;
    use std::str::FromStr;
    Cron::from_str(expr)
        .map_err(|e| AcsError::Cron(format!("Invalid cron expression '{}': {}", expr, e)))?;
    Ok(())
}

fn validate_timezone(tz: &str) -> Result<(), AcsError> {
    tz.parse::<chrono_tz::Tz>()
        .map_err(|e| AcsError::Validation(format!("Invalid timezone '{}': {}", tz, e)))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_new_job() -> NewJob {
        NewJob {
            name: "test-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
        }
    }

    fn make_job() -> Job {
        let now = Utc::now();
        Job {
            id: Uuid::now_v7(),
            name: "test-job".to_string(),
            schedule: "*/5 * * * *".to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled: true,
            timezone: Some("America/New_York".to_string()),
            working_dir: Some("/tmp".to_string()),
            env_vars: Some({
                let mut m = HashMap::new();
                m.insert("FOO".to_string(), "bar".to_string());
                m
            }),
            timeout_secs: 0,
            log_environment: false,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        }
    }

    #[test]
    fn test_job_serde_roundtrip() {
        let job = make_job();
        let json = serde_json::to_string(&job).expect("serialize");
        let deserialized: Job = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(job, deserialized);
    }

    #[test]
    fn test_job_next_run_at_skipped_in_serde() {
        let mut job = make_job();
        job.next_run_at = Some(Utc::now());
        let json = serde_json::to_string(&job).expect("serialize");
        let deserialized: Job = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.next_run_at.is_none());
    }

    #[test]
    fn test_validation_empty_name_rejected() {
        let mut job = make_new_job();
        job.name = "".to_string();
        let result = validate_new_job(&job);
        assert!(result.is_err());
        match result.unwrap_err() {
            AcsError::Validation(msg) => assert!(msg.contains("empty")),
            other => panic!("Expected Validation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validation_whitespace_name_rejected() {
        let mut job = make_new_job();
        job.name = "   ".to_string();
        let result = validate_new_job(&job);
        assert!(result.is_err());
    }

    #[test]
    fn test_validation_uuid_as_name_rejected() {
        let mut job = make_new_job();
        job.name = Uuid::now_v7().to_string();
        let result = validate_new_job(&job);
        assert!(result.is_err());
        match result.unwrap_err() {
            AcsError::Validation(msg) => assert!(msg.contains("UUID")),
            other => panic!("Expected Validation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validation_invalid_cron_rejected() {
        let mut job = make_new_job();
        job.schedule = "not a cron".to_string();
        let result = validate_new_job(&job);
        assert!(result.is_err());
        match result.unwrap_err() {
            AcsError::Cron(_) => {}
            other => panic!("Expected Cron, got: {:?}", other),
        }
    }

    #[test]
    fn test_validation_invalid_timezone_rejected() {
        let mut job = make_new_job();
        job.timezone = Some("Not/A/Timezone".to_string());
        let result = validate_new_job(&job);
        assert!(result.is_err());
        match result.unwrap_err() {
            AcsError::Validation(msg) => assert!(msg.contains("timezone")),
            other => panic!("Expected Validation, got: {:?}", other),
        }
    }

    #[test]
    fn test_validation_valid_job_succeeds() {
        let job = make_new_job();
        assert!(validate_new_job(&job).is_ok());
    }

    #[test]
    fn test_validation_valid_job_with_timezone_succeeds() {
        let mut job = make_new_job();
        job.timezone = Some("America/New_York".to_string());
        assert!(validate_new_job(&job).is_ok());
    }

    #[test]
    fn test_execution_type_shell_command_serde() {
        let exec = ExecutionType::ShellCommand("echo hello".to_string());
        let json = serde_json::to_string(&exec).expect("serialize");
        assert!(json.contains("\"type\":\"ShellCommand\""));
        assert!(json.contains("\"value\":\"echo hello\""));
        let deserialized: ExecutionType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(exec, deserialized);
    }

    #[test]
    fn test_execution_type_script_file_serde() {
        let exec = ExecutionType::ScriptFile("deploy.sh".to_string());
        let json = serde_json::to_string(&exec).expect("serialize");
        assert!(json.contains("\"type\":\"ScriptFile\""));
        assert!(json.contains("\"value\":\"deploy.sh\""));
        let deserialized: ExecutionType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(exec, deserialized);
    }

    #[test]
    fn test_new_job_serde_roundtrip() {
        let job = make_new_job();
        let json = serde_json::to_string(&job).expect("serialize");
        let deserialized: NewJob = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(job.name, deserialized.name);
        assert_eq!(job.schedule, deserialized.schedule);
        assert_eq!(job.execution, deserialized.execution);
        assert_eq!(job.enabled, deserialized.enabled);
    }

    #[test]
    fn test_new_job_default_enabled() {
        let json = r#"{"name":"test","schedule":"* * * * *","execution":{"type":"ShellCommand","value":"echo hi"}}"#;
        let job: NewJob = serde_json::from_str(json).expect("deserialize");
        assert!(job.enabled);
    }

    #[test]
    fn test_job_update_all_none() {
        let update = JobUpdate::default();
        assert!(update.name.is_none());
        assert!(update.schedule.is_none());
        assert!(update.execution.is_none());
        assert!(update.enabled.is_none());
        assert!(update.timezone.is_none());
        assert!(update.working_dir.is_none());
        assert!(update.env_vars.is_none());
    }

    #[test]
    fn test_job_update_serde_partial() {
        let json = r#"{"name":"updated-name"}"#;
        let update: JobUpdate = serde_json::from_str(json).expect("deserialize");
        assert_eq!(update.name, Some("updated-name".to_string()));
        assert!(update.schedule.is_none());
    }

    #[test]
    fn test_validate_job_update_empty_name() {
        let update = JobUpdate {
            name: Some("".to_string()),
            ..Default::default()
        };
        assert!(validate_job_update(&update).is_err());
    }

    #[test]
    fn test_validate_job_update_uuid_name() {
        let update = JobUpdate {
            name: Some(Uuid::now_v7().to_string()),
            ..Default::default()
        };
        assert!(validate_job_update(&update).is_err());
    }

    #[test]
    fn test_validate_job_update_invalid_cron() {
        let update = JobUpdate {
            schedule: Some("bad cron".to_string()),
            ..Default::default()
        };
        assert!(validate_job_update(&update).is_err());
    }

    #[test]
    fn test_validate_job_update_invalid_timezone() {
        let update = JobUpdate {
            timezone: Some("Invalid/TZ".to_string()),
            ..Default::default()
        };
        assert!(validate_job_update(&update).is_err());
    }

    #[test]
    fn test_validate_job_update_valid() {
        let update = JobUpdate {
            name: Some("new-name".to_string()),
            schedule: Some("0 * * * *".to_string()),
            timezone: Some("Europe/London".to_string()),
            ..Default::default()
        };
        assert!(validate_job_update(&update).is_ok());
    }
}
