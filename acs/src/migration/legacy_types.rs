//! Legacy types used only by the migration path.
//!
//! These structs describe the on-disk shape of `jobs.json` from the pre-Phase 6
//! era. They are kept here so the migration code can deserialize old files.
//! Nothing outside of the `migration` module should reference these types.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::workflow::ScheduleMode;

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
    #[serde(default)]
    pub allow_concurrent: bool,
    #[serde(default)]
    pub schedule_mode: ScheduleMode,
    #[serde(default)]
    pub pre_hook: Option<String>,
    #[serde(default)]
    pub post_hook: Option<String>,
    #[serde(default)]
    pub pre_hook_script_type: Option<String>,
    #[serde(default)]
    pub post_hook_script_type: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_run_at: Option<DateTime<Utc>>,
    pub last_exit_code: Option<i32>,
    #[serde(skip_deserializing, default)]
    pub next_run_at: Option<DateTime<Utc>>,
}
