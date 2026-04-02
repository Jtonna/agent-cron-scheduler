use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::JobRun;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct ModelUsageBucket {
    pub runs: u64,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_input_tokens: u64,
    pub cache_read_input_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(default)]
pub struct TimeBucket {
    pub runs: u64,
    pub cost_usd: f64,
    pub duration_ms: u64,
    pub num_turns: u64,
    pub models: BTreeMap<String, ModelUsageBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct JobManifest {
    pub job_id: Uuid,
    pub version: u32,
    pub updated_at: DateTime<Utc>,
    pub total_runs: u64,
    pub total_cost_usd: f64,
    pub total_duration_ms: u64,
    pub daily_buckets: BTreeMap<String, TimeBucket>,
    pub weekly_buckets: BTreeMap<String, TimeBucket>,
    pub monthly_buckets: BTreeMap<String, TimeBucket>,
}

impl Default for JobManifest {
    fn default() -> Self {
        Self {
            job_id: Uuid::nil(),
            version: 1,
            updated_at: Utc::now(),
            total_runs: 0,
            total_cost_usd: 0.0,
            total_duration_ms: 0,
            daily_buckets: BTreeMap::new(),
            weekly_buckets: BTreeMap::new(),
            monthly_buckets: BTreeMap::new(),
        }
    }
}

impl JobManifest {
    pub fn new(job_id: Uuid) -> Self {
        Self {
            job_id,
            version: 1,
            updated_at: Utc::now(),
            total_runs: 0,
            total_cost_usd: 0.0,
            total_duration_ms: 0,
            daily_buckets: BTreeMap::new(),
            weekly_buckets: BTreeMap::new(),
            monthly_buckets: BTreeMap::new(),
        }
    }

    pub fn merge_run(&mut self, run: &JobRun) {
        let cost = run.total_cost_usd.unwrap_or(0.0);
        let duration = run.duration_ms.unwrap_or(0);
        let turns = run.num_turns.unwrap_or(0) as u64;

        self.total_runs += 1;
        self.total_cost_usd += cost;
        self.total_duration_ms += duration;

        let daily_key = run.started_at.format("%Y-%m-%d").to_string();
        let weekly_key = run.started_at.format("%G-W%V").to_string();
        let monthly_key = run.started_at.format("%Y-%m").to_string();

        for bucket in [
            self.daily_buckets.entry(daily_key).or_default(),
            self.weekly_buckets.entry(weekly_key).or_default(),
            self.monthly_buckets.entry(monthly_key).or_default(),
        ] {
            bucket.runs += 1;
            bucket.cost_usd += cost;
            bucket.duration_ms += duration;
            bucket.num_turns += turns;

            if let Some(model_name) = &run.model {
                let model_entry = bucket.models.entry(model_name.clone()).or_default();
                model_entry.runs += 1;
                model_entry.cost_usd += cost;

                if let Some(usage) = &run.usage {
                    model_entry.input_tokens +=
                        usage.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    model_entry.output_tokens +=
                        usage.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0);
                    model_entry.cache_creation_input_tokens += usage
                        .get("cache_creation_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    model_entry.cache_read_input_tokens += usage
                        .get("cache_read_input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                }
            }
        }

        self.updated_at = Utc::now();
    }
}
