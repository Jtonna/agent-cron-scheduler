use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::job::ScheduleMode;

// ─── Workflow ────────────────────────────────────────────────────────────────

fn default_allow_concurrent() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: Uuid,
    pub name: String,
    pub version: u32,
    pub schedule: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub schedule_mode: ScheduleMode,
    pub enabled: bool,
    pub steps: Vec<StepDef>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub default_input: Option<serde_json::Value>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env_vars: Option<HashMap<String, String>>,
    #[serde(default = "default_allow_concurrent")]
    pub allow_concurrent: bool,
    #[serde(default)]
    pub on_failure: FailurePolicy,
    #[serde(default)]
    pub last_run_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_run_status: Option<RunStatus>,
    #[serde(default)]
    pub last_run_id: Option<Uuid>,
    #[serde(skip_deserializing, default)]
    pub next_run_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl PartialEq for Workflow {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.version == other.version
            && self.schedule == other.schedule
            && self.timezone == other.timezone
            && self.schedule_mode == other.schedule_mode
            && self.enabled == other.enabled
            && self.steps == other.steps
            && self.input_schema == other.input_schema
            && self.default_input == other.default_input
            && self.working_dir == other.working_dir
            && self.env_vars == other.env_vars
            && self.allow_concurrent == other.allow_concurrent
            && self.on_failure == other.on_failure
            && self.last_run_at == other.last_run_at
            && self.last_run_status == other.last_run_status
            && self.last_run_id == other.last_run_id
            && self.created_at == other.created_at
            && self.updated_at == other.updated_at
        // next_run_at is skipped (computed, not persisted)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NewWorkflow {
    pub name: String,
    pub schedule: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub schedule_mode: ScheduleMode,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    pub steps: Vec<StepDef>,
    #[serde(default)]
    pub input_schema: Option<serde_json::Value>,
    #[serde(default)]
    pub default_input: Option<serde_json::Value>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env_vars: Option<HashMap<String, String>>,
    pub allow_concurrent: Option<bool>,
    #[serde(default)]
    pub on_failure: FailurePolicy,
}

fn default_enabled() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct WorkflowUpdate {
    pub name: Option<String>,
    pub schedule: Option<String>,
    pub timezone: Option<String>,
    pub schedule_mode: Option<ScheduleMode>,
    pub enabled: Option<bool>,
    pub steps: Option<Vec<StepDef>>,
    pub input_schema: Option<serde_json::Value>,
    pub default_input: Option<serde_json::Value>,
    pub working_dir: Option<String>,
    pub env_vars: Option<HashMap<String, String>>,
    pub allow_concurrent: Option<bool>,
    pub on_failure: Option<FailurePolicy>,
}

// ─── StepDef ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum StepDef {
    Shell(ShellStep),
    Script(ScriptStep),
    Http(HttpStep),
    Match(MatchStep),
    SetVar(SetVarStep),
    Agent(AgentStep),
}

// ─── StepDefCommon ────────────────────────────────────────────────────────────

fn default_stdout_max_bytes() -> usize {
    65536 // 64 KB
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureSpec {
    #[serde(default = "default_stdout_max_bytes")]
    pub stdout_max_bytes: usize,
    #[serde(default)]
    pub parser: Option<String>,
}

impl Default for CaptureSpec {
    fn default() -> Self {
        CaptureSpec {
            stdout_max_bytes: default_stdout_max_bytes(),
            parser: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepDefCommon {
    pub id: String,
    #[serde(default)]
    pub on_failure: Option<FailurePolicy>,
    #[serde(default)]
    pub always_run: bool,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub env_vars: Option<HashMap<String, String>>,
    #[serde(default)]
    pub capture: CaptureSpec,
}

// ─── Step variant structs ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShellStep {
    #[serde(flatten)]
    pub common: StepDefCommon,
    pub command: String,
    #[serde(default)]
    pub pass_stdin: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScriptStep {
    #[serde(flatten)]
    pub common: StepDefCommon,
    pub path: String,
    #[serde(default)]
    pub script_type: Option<String>,
    #[serde(default)]
    pub args: Option<String>,
    #[serde(default)]
    pub pass_stdin: bool,
}

fn default_expect_status() -> Vec<u16> {
    (200u16..300u16).collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HttpStep {
    #[serde(flatten)]
    pub common: StepDefCommon,
    pub method: String,
    pub url: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default = "default_expect_status")]
    pub expect_status: Vec<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MatchStep {
    #[serde(flatten)]
    pub common: StepDefCommon,
    pub expr: String,
    pub cases: HashMap<String, Vec<StepDef>>,
    #[serde(default)]
    pub default: Option<Vec<StepDef>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SetVarStep {
    #[serde(flatten)]
    pub common: StepDefCommon,
    pub exports: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentStep {
    #[serde(flatten)]
    pub common: StepDefCommon,
    pub agent_type: AgentType,
    pub prompt: String,
    #[serde(default)]
    pub command_template: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentType {
    ClaudeCodeCli,
}

// ─── Failure / status ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum FailurePolicy {
    #[default]
    Abort,
    Continue,
    Retry {
        attempts: u32,
        backoff_ms: u64,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub enum RunStatus {
    Running,
    Completed,
    Failed,
    Killed,
}

// ─── Run records ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkflowRun {
    pub run_id: Uuid,
    pub workflow_id: Uuid,
    pub workflow_version: u32,
    pub workflow_snapshot: Workflow,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    pub status: RunStatus,
    #[serde(default)]
    pub trigger_input: Option<serde_json::Value>,
    pub steps: Vec<StepRun>,
    #[serde(default)]
    pub total_cost_usd: Option<f64>,
    #[serde(default)]
    pub total_duration_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StepRun {
    pub step_index: usize,
    pub step_id: String,
    pub kind: String,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    #[serde(default)]
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub exit_code: Option<i32>,
    pub log_byte_offset_start: u64,
    #[serde(default)]
    pub log_byte_offset_end: Option<u64>,
    #[serde(default)]
    pub cost_usd: Option<f64>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub output_summary: Option<serde_json::Value>,
}

// ─── TriggerParams (workflow variant) ────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TriggerParams {
    pub input: serde_json::Value,
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
    #[serde(default)]
    pub target_step: Option<String>,
}

// ─── Validators ───────────────────────────────────────────────────────────────

pub fn validate_new_workflow(wf: &NewWorkflow) -> Result<(), AcsError> {
    validate_name(&wf.name)?;
    validate_cron(&wf.schedule)?;
    if let Some(ref tz) = wf.timezone {
        validate_timezone(tz)?;
    }
    if wf.steps.is_empty() {
        return Err(AcsError::Validation(
            "Workflow must have at least one step".to_string(),
        ));
    }
    validate_step_ids_unique(&wf.steps)?;
    for step in &wf.steps {
        validate_capture_spec(step)?;
    }
    Ok(())
}

pub fn validate_workflow_update(update: &WorkflowUpdate) -> Result<(), AcsError> {
    if let Some(ref name) = update.name {
        validate_name(name)?;
    }
    if let Some(ref schedule) = update.schedule {
        validate_cron(schedule)?;
    }
    if let Some(ref tz) = update.timezone {
        validate_timezone(tz)?;
    }
    if let Some(ref steps) = update.steps {
        if steps.is_empty() {
            return Err(AcsError::Validation(
                "Workflow must have at least one step".to_string(),
            ));
        }
        validate_step_ids_unique(steps)?;
        for step in steps {
            validate_capture_spec(step)?;
        }
    }
    Ok(())
}

/// Validate the `version` field on an existing Workflow (must be >= 1).
pub fn validate_workflow_version(wf: &Workflow) -> Result<(), AcsError> {
    if wf.version < 1 {
        return Err(AcsError::Validation(
            "Workflow version must be >= 1".to_string(),
        ));
    }
    Ok(())
}

fn validate_name(name: &str) -> Result<(), AcsError> {
    if name.trim().is_empty() {
        return Err(AcsError::Validation(
            "Workflow name cannot be empty".to_string(),
        ));
    }
    if Uuid::parse_str(name).is_ok() {
        return Err(AcsError::Validation(
            "Workflow name cannot be a valid UUID".to_string(),
        ));
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

fn validate_step_ids_unique(steps: &[StepDef]) -> Result<(), AcsError> {
    let mut seen = std::collections::HashSet::new();
    collect_step_ids(steps, &mut seen)?;
    Ok(())
}

fn collect_step_ids<'a>(
    steps: &'a [StepDef],
    seen: &mut std::collections::HashSet<&'a str>,
) -> Result<(), AcsError> {
    for step in steps {
        let id = step_common(step).id.as_str();
        if !seen.insert(id) {
            return Err(AcsError::Validation(format!(
                "step id '{}' is duplicated",
                id
            )));
        }
        // Recurse into MatchStep branches
        if let StepDef::Match(m) = step {
            for branch in m.cases.values() {
                collect_step_ids(branch, seen)?;
            }
            if let Some(ref default_steps) = m.default {
                collect_step_ids(default_steps, seen)?;
            }
        }
    }
    Ok(())
}

fn step_common(step: &StepDef) -> &StepDefCommon {
    match step {
        StepDef::Shell(s) => &s.common,
        StepDef::Script(s) => &s.common,
        StepDef::Http(s) => &s.common,
        StepDef::Match(s) => &s.common,
        StepDef::SetVar(s) => &s.common,
        StepDef::Agent(s) => &s.common,
    }
}

fn validate_capture_spec(step: &StepDef) -> Result<(), AcsError> {
    validate_one_capture(&step_common(step).capture)?;
    // Recurse into MatchStep branches
    if let StepDef::Match(m) = step {
        for branch in m.cases.values() {
            for s in branch {
                validate_capture_spec(s)?;
            }
        }
        if let Some(ref default_steps) = m.default {
            for s in default_steps {
                validate_capture_spec(s)?;
            }
        }
    }
    Ok(())
}

fn validate_one_capture(cap: &CaptureSpec) -> Result<(), AcsError> {
    if let Some(ref parser) = cap.parser {
        match parser.as_str() {
            "json" | "lines" | "raw" => {}
            other => {
                return Err(AcsError::Validation(format!(
                    "Invalid capture parser '{}': must be 'json', 'lines', or 'raw'",
                    other
                )));
            }
        }
    }
    Ok(())
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_shell_step(id: &str) -> StepDef {
        StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(30),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: "echo hello".to_string(),
            pass_stdin: false,
        })
    }

    fn make_workflow() -> Workflow {
        let now = Utc::now();
        Workflow {
            id: Uuid::now_v7(),
            name: "my-workflow".to_string(),
            version: 1,
            schedule: "*/5 * * * *".to_string(),
            timezone: Some("America/New_York".to_string()),
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            input_schema: None,
            default_input: None,
            working_dir: Some("/tmp".to_string()),
            env_vars: Some({
                let mut m = HashMap::new();
                m.insert("FOO".to_string(), "bar".to_string());
                m
            }),
            allow_concurrent: true,
            on_failure: FailurePolicy::Abort,
            last_run_at: None,
            last_run_status: None,
            last_run_id: None,
            next_run_at: None,
            created_at: now,
            updated_at: now,
        }
    }

    fn make_new_workflow() -> NewWorkflow {
        NewWorkflow {
            name: "my-workflow".to_string(),
            schedule: "*/5 * * * *".to_string(),
            timezone: None,
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            steps: vec![make_shell_step("step-1")],
            input_schema: None,
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: None,
            on_failure: FailurePolicy::default(),
        }
    }

    // ── Workflow serde round-trip ──────────────────────────────────────────────

    #[test]
    fn test_workflow_serde_roundtrip() {
        let wf = make_workflow();
        let json = serde_json::to_string(&wf).expect("serialize");
        let deserialized: Workflow = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(wf, deserialized);
    }

    #[test]
    fn test_workflow_next_run_at_skipped_in_serde() {
        let mut wf = make_workflow();
        wf.next_run_at = Some(Utc::now());
        let json = serde_json::to_string(&wf).expect("serialize");
        let deserialized: Workflow = serde_json::from_str(&json).expect("deserialize");
        assert!(deserialized.next_run_at.is_none());
    }

    #[test]
    fn test_workflow_allow_concurrent_defaults_to_true() {
        // JSON without allow_concurrent — should default to true
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "my-workflow",
            "version": 1,
            "schedule": "* * * * *",
            "enabled": true,
            "steps": [{"kind":"shell","id":"s1","command":"echo hi"}],
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        }"#;
        let wf: Workflow = serde_json::from_str(json).expect("deserialize");
        assert!(wf.allow_concurrent);
    }

    // ── StepDef serde per variant ─────────────────────────────────────────────

    #[test]
    fn test_step_def_shell_serde() {
        let step = make_shell_step("step-a");
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(json.contains("\"kind\":\"shell\""), "json: {}", json);
        let deserialized: StepDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(step, deserialized);
    }

    #[test]
    fn test_step_def_script_serde() {
        let step = StepDef::Script(ScriptStep {
            common: StepDefCommon {
                id: "script-step".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            path: "/usr/local/bin/deploy.sh".to_string(),
            script_type: Some("bash".to_string()),
            args: Some("--verbose".to_string()),
            pass_stdin: false,
        });
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(json.contains("\"kind\":\"script\""), "json: {}", json);
        let deserialized: StepDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(step, deserialized);
    }

    #[test]
    fn test_step_def_http_serde() {
        let mut headers = HashMap::new();
        headers.insert("Authorization".to_string(), "Bearer token".to_string());
        let step = StepDef::Http(HttpStep {
            common: StepDefCommon {
                id: "http-step".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(10),
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            method: "POST".to_string(),
            url: "https://example.com/api".to_string(),
            headers,
            body: Some("{\"key\":\"value\"}".to_string()),
            expect_status: vec![200, 201],
        });
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(json.contains("\"kind\":\"http\""), "json: {}", json);
        let deserialized: StepDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(step, deserialized);
    }

    #[test]
    fn test_step_def_match_serde() {
        let mut cases = HashMap::new();
        cases.insert("success".to_string(), vec![make_shell_step("success-step")]);
        cases.insert("failure".to_string(), vec![make_shell_step("failure-step")]);
        let step = StepDef::Match(MatchStep {
            common: StepDefCommon {
                id: "match-step".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            expr: "${last_exit_code}".to_string(),
            cases,
            default: Some(vec![make_shell_step("default-step")]),
        });
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(json.contains("\"kind\":\"match\""), "json: {}", json);
        let deserialized: StepDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(step, deserialized);
    }

    #[test]
    fn test_step_def_setvar_serde() {
        let mut exports = HashMap::new();
        exports.insert("MY_VAR".to_string(), "some_value".to_string());
        let step = StepDef::SetVar(SetVarStep {
            common: StepDefCommon {
                id: "setvar-step".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            exports,
        });
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(json.contains("\"kind\":\"set_var\""), "json: {}", json);
        let deserialized: StepDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(step, deserialized);
    }

    #[test]
    fn test_step_def_agent_serde() {
        let step = StepDef::Agent(AgentStep {
            common: StepDefCommon {
                id: "agent-step".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: Some(120),
                working_dir: Some("/workspace".to_string()),
                env_vars: None,
                capture: CaptureSpec {
                    stdout_max_bytes: 65536,
                    parser: Some("json".to_string()),
                },
            },
            agent_type: AgentType::ClaudeCodeCli,
            prompt: "Analyze this code and provide feedback.".to_string(),
            command_template: Some("claude --output-format json".to_string()),
        });
        let json = serde_json::to_string(&step).expect("serialize");
        assert!(json.contains("\"kind\":\"agent\""), "json: {}", json);
        let deserialized: StepDef = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(step, deserialized);
    }

    // ── StepDefCommon flatten ─────────────────────────────────────────────────

    #[test]
    fn test_step_def_common_flatten() {
        let step = ShellStep {
            common: StepDefCommon {
                id: "flat-test".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: "echo world".to_string(),
            pass_stdin: true,
        };
        let json = serde_json::to_string(&step).expect("serialize");
        let val: serde_json::Value = serde_json::from_str(&json).expect("parse");
        // id, command, pass_stdin must be at top level — no "common" key
        assert!(val.get("id").is_some(), "expected top-level 'id'");
        assert!(val.get("command").is_some(), "expected top-level 'command'");
        assert!(val.get("pass_stdin").is_some(), "expected top-level 'pass_stdin'");
        assert!(val.get("common").is_none(), "unexpected nested 'common' key");
    }

    // ── RunStatus serde ───────────────────────────────────────────────────────

    #[test]
    fn test_run_status_running_serde() {
        let s = RunStatus::Running;
        let json = serde_json::to_string(&s).expect("serialize");
        assert_eq!(json, "\"Running\"");
        let d: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, d);
    }

    #[test]
    fn test_run_status_completed_serde() {
        let s = RunStatus::Completed;
        let json = serde_json::to_string(&s).expect("serialize");
        assert_eq!(json, "\"Completed\"");
        let d: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, d);
    }

    #[test]
    fn test_run_status_failed_serde() {
        let s = RunStatus::Failed;
        let json = serde_json::to_string(&s).expect("serialize");
        assert_eq!(json, "\"Failed\"");
        let d: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, d);
    }

    #[test]
    fn test_run_status_killed_serde() {
        let s = RunStatus::Killed;
        let json = serde_json::to_string(&s).expect("serialize");
        assert_eq!(json, "\"Killed\"");
        let d: RunStatus = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(s, d);
    }

    // ── FailurePolicy serde ───────────────────────────────────────────────────

    #[test]
    fn test_failure_policy_abort_serde() {
        let p = FailurePolicy::Abort;
        let json = serde_json::to_string(&p).expect("serialize");
        assert_eq!(json, "\"abort\"");
        let d: FailurePolicy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, d);
    }

    #[test]
    fn test_failure_policy_continue_serde() {
        let p = FailurePolicy::Continue;
        let json = serde_json::to_string(&p).expect("serialize");
        assert_eq!(json, "\"continue\"");
        let d: FailurePolicy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, d);
    }

    #[test]
    fn test_failure_policy_retry_serde() {
        let p = FailurePolicy::Retry {
            attempts: 3,
            backoff_ms: 1000,
        };
        let json = serde_json::to_string(&p).expect("serialize");
        // Tagged map — should contain "retry" and the struct fields
        let val: serde_json::Value = serde_json::from_str(&json).expect("parse");
        assert!(
            val.get("retry").is_some() || json.contains("retry"),
            "expected retry in json: {}",
            json
        );
        let d: FailurePolicy = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(p, d);
    }

    // ── AgentType serde ───────────────────────────────────────────────────────

    #[test]
    fn test_agent_type_claude_code_cli_serde() {
        let t = AgentType::ClaudeCodeCli;
        let json = serde_json::to_string(&t).expect("serialize");
        assert_eq!(json, "\"claude_code_cli\"");
        let d: AgentType = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(t, d);
    }

    // ── WorkflowRun round-trip ────────────────────────────────────────────────

    #[test]
    fn test_workflow_run_serde_roundtrip() {
        let now = Utc::now();
        let wf = make_workflow();
        let run = WorkflowRun {
            run_id: Uuid::now_v7(),
            workflow_id: wf.id,
            workflow_version: wf.version,
            workflow_snapshot: wf,
            started_at: now,
            finished_at: Some(now),
            status: RunStatus::Completed,
            trigger_input: Some(serde_json::json!({"key": "value"})),
            steps: vec![StepRun {
                step_index: 0,
                step_id: "step-1".to_string(),
                kind: "shell".to_string(),
                status: RunStatus::Completed,
                started_at: now,
                finished_at: Some(now),
                exit_code: Some(0),
                log_byte_offset_start: 0,
                log_byte_offset_end: Some(1024),
                cost_usd: None,
                error: None,
                output_summary: None,
            }],
            total_cost_usd: Some(0.001),
            total_duration_ms: Some(500),
        };
        let json = serde_json::to_string(&run).expect("serialize");
        let deserialized: WorkflowRun = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(run, deserialized);
    }

    // ── Validators ────────────────────────────────────────────────────────────

    #[test]
    fn test_validate_empty_name_rejected() {
        let mut wf = make_new_workflow();
        wf.name = "".to_string();
        assert!(matches!(
            validate_new_workflow(&wf),
            Err(AcsError::Validation(_))
        ));
    }

    #[test]
    fn test_validate_whitespace_name_rejected() {
        let mut wf = make_new_workflow();
        wf.name = "   ".to_string();
        assert!(matches!(
            validate_new_workflow(&wf),
            Err(AcsError::Validation(_))
        ));
    }

    #[test]
    fn test_validate_uuid_name_rejected() {
        let mut wf = make_new_workflow();
        wf.name = Uuid::now_v7().to_string();
        let result = validate_new_workflow(&wf);
        assert!(matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("UUID")));
    }

    #[test]
    fn test_validate_invalid_cron_rejected() {
        let mut wf = make_new_workflow();
        wf.schedule = "not a cron".to_string();
        assert!(matches!(validate_new_workflow(&wf), Err(AcsError::Cron(_))));
    }

    #[test]
    fn test_validate_invalid_timezone_rejected() {
        let mut wf = make_new_workflow();
        wf.timezone = Some("Not/A/Timezone".to_string());
        let result = validate_new_workflow(&wf);
        assert!(matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("timezone")));
    }

    #[test]
    fn test_validate_empty_steps_rejected() {
        let mut wf = make_new_workflow();
        wf.steps = vec![];
        let result = validate_new_workflow(&wf);
        assert!(matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("step")));
    }

    #[test]
    fn test_validate_duplicate_step_id_rejected() {
        let mut wf = make_new_workflow();
        wf.steps = vec![make_shell_step("dup"), make_shell_step("dup")];
        let result = validate_new_workflow(&wf);
        assert!(
            matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("duplicated")),
            "expected duplicate step id error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_duplicate_step_id_in_match_cases_rejected() {
        // "dup" appears at top level and inside a MatchStep branch
        let mut cases = HashMap::new();
        cases.insert("branch".to_string(), vec![make_shell_step("dup")]);
        let match_step = StepDef::Match(MatchStep {
            common: StepDefCommon {
                id: "match-1".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            expr: "${x}".to_string(),
            cases,
            default: None,
        });
        let mut wf = make_new_workflow();
        // top-level step id "dup", match step id "match-1", nested step id "dup" — duplicate
        wf.steps = vec![make_shell_step("dup"), match_step];
        let result = validate_new_workflow(&wf);
        assert!(
            matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("duplicated")),
            "expected duplicate step id error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_duplicate_step_id_in_match_default_rejected() {
        // "dup" appears in default branch and cases branch
        let mut cases = HashMap::new();
        cases.insert("x".to_string(), vec![make_shell_step("dup")]);
        let match_step = StepDef::Match(MatchStep {
            common: StepDefCommon {
                id: "match-1".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            expr: "${x}".to_string(),
            cases,
            default: Some(vec![make_shell_step("dup")]),
        });
        let mut wf = make_new_workflow();
        wf.steps = vec![match_step];
        let result = validate_new_workflow(&wf);
        assert!(
            matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("duplicated")),
            "expected duplicate step id error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_invalid_capture_parser_rejected() {
        let mut wf = make_new_workflow();
        wf.steps = vec![StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: "step-bad-parser".to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec {
                    stdout_max_bytes: 65536,
                    parser: Some("xml".to_string()),
                },
            },
            command: "echo hi".to_string(),
            pass_stdin: false,
        })];
        let result = validate_new_workflow(&wf);
        assert!(
            matches!(result, Err(AcsError::Validation(ref msg)) if msg.contains("parser")),
            "expected parser validation error, got: {:?}",
            result
        );
    }

    #[test]
    fn test_validate_valid_capture_parsers_accepted() {
        for parser in &["json", "lines", "raw"] {
            let mut wf = make_new_workflow();
            wf.steps = vec![StepDef::Shell(ShellStep {
                common: StepDefCommon {
                    id: "step-parser".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec {
                        stdout_max_bytes: 65536,
                        parser: Some(parser.to_string()),
                    },
                },
                command: "echo hi".to_string(),
                pass_stdin: false,
            })];
            assert!(
                validate_new_workflow(&wf).is_ok(),
                "parser '{}' should be valid",
                parser
            );
        }
    }

    #[test]
    fn test_validate_valid_workflow_accepted() {
        let wf = make_new_workflow();
        assert!(validate_new_workflow(&wf).is_ok());
    }

    #[test]
    fn test_validate_valid_workflow_with_timezone_accepted() {
        let mut wf = make_new_workflow();
        wf.timezone = Some("America/New_York".to_string());
        assert!(validate_new_workflow(&wf).is_ok());
    }

    #[test]
    fn test_validate_workflow_version_less_than_one_rejected() {
        let mut wf = make_workflow();
        wf.version = 0;
        assert!(matches!(
            validate_workflow_version(&wf),
            Err(AcsError::Validation(_))
        ));
    }

    #[test]
    fn test_validate_workflow_version_one_accepted() {
        let wf = make_workflow();
        assert_eq!(wf.version, 1);
        assert!(validate_workflow_version(&wf).is_ok());
    }

    #[test]
    fn test_validate_workflow_update_empty_name_rejected() {
        let update = WorkflowUpdate {
            name: Some("".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            validate_workflow_update(&update),
            Err(AcsError::Validation(_))
        ));
    }

    #[test]
    fn test_validate_workflow_update_invalid_cron_rejected() {
        let update = WorkflowUpdate {
            schedule: Some("bad cron".to_string()),
            ..Default::default()
        };
        assert!(matches!(
            validate_workflow_update(&update),
            Err(AcsError::Cron(_))
        ));
    }

    #[test]
    fn test_validate_workflow_update_valid() {
        let update = WorkflowUpdate {
            name: Some("new-workflow-name".to_string()),
            schedule: Some("0 * * * *".to_string()),
            timezone: Some("Europe/London".to_string()),
            ..Default::default()
        };
        assert!(validate_workflow_update(&update).is_ok());
    }

    // ── allow_concurrent default ──────────────────────────────────────────────

    #[test]
    fn test_workflow_allow_concurrent_serialized_field_false_is_false() {
        let json = r#"{
            "id": "550e8400-e29b-41d4-a716-446655440000",
            "name": "my-workflow",
            "version": 1,
            "schedule": "* * * * *",
            "enabled": true,
            "steps": [{"kind":"shell","id":"s1","command":"echo hi"}],
            "allow_concurrent": false,
            "created_at": "2025-01-01T00:00:00Z",
            "updated_at": "2025-01-01T00:00:00Z"
        }"#;
        let wf: Workflow = serde_json::from_str(json).expect("deserialize");
        assert!(!wf.allow_concurrent);
    }
}
