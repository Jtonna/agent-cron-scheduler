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
    #[serde(default)]
    pub runs_by_status: BTreeMap<String, u64>,
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

            let status_key = format!("{:?}", run.status);
            *bucket.runs_by_status.entry(status_key).or_insert(0) += 1;

            if let Some(model_name) = &run.model {
                let model_entry = bucket.models.entry(model_name.clone()).or_default();
                model_entry.runs += 1;
                model_entry.cost_usd += cost;

                if let Some(usage) = &run.usage {
                    model_entry.input_tokens += usage
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    model_entry.output_tokens += usage
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::RunStatus;
    use chrono::TimeZone;

    fn make_test_run(
        job_id: Uuid,
        started_at: DateTime<Utc>,
        cost: Option<f64>,
        model: Option<&str>,
        usage: Option<serde_json::Value>,
    ) -> JobRun {
        make_test_run_with_status(job_id, started_at, cost, model, usage, RunStatus::Completed)
    }

    fn make_test_run_with_status(
        job_id: Uuid,
        started_at: DateTime<Utc>,
        cost: Option<f64>,
        model: Option<&str>,
        usage: Option<serde_json::Value>,
        status: RunStatus,
    ) -> JobRun {
        crate::models::JobRun {
            run_id: Uuid::now_v7(),
            job_id,
            started_at,
            finished_at: Some(started_at + chrono::Duration::seconds(60)),
            status,
            exit_code: Some(0),
            log_size_bytes: 512,
            error: None,
            trigger_params: None,
            total_cost_usd: cost,
            duration_ms: cost.map(|_| 1000),
            num_turns: Some(3),
            model: model.map(|s| s.to_string()),
            usage,
        }
    }

    #[test]
    fn test_serialization_roundtrip() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        let usage = serde_json::json!({
            "input_tokens": 1000_u64,
            "output_tokens": 500_u64,
            "cache_creation_input_tokens": 200_u64,
            "cache_read_input_tokens": 300_u64
        });
        let run = make_test_run(
            job_id,
            dt,
            Some(0.50),
            Some("claude-sonnet-4-20250514"),
            Some(usage),
        );
        manifest.merge_run(&run);

        let json = serde_json::to_string(&manifest).expect("serialize");
        let deserialized: JobManifest = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(manifest.job_id, deserialized.job_id);
        assert_eq!(manifest.version, deserialized.version);
        assert_eq!(manifest.total_runs, deserialized.total_runs);
        assert_eq!(manifest.total_cost_usd, deserialized.total_cost_usd);
        assert_eq!(manifest.total_duration_ms, deserialized.total_duration_ms);
        assert_eq!(manifest.daily_buckets, deserialized.daily_buckets);
        assert_eq!(manifest.weekly_buckets, deserialized.weekly_buckets);
        assert_eq!(manifest.monthly_buckets, deserialized.monthly_buckets);
    }

    #[test]
    fn test_merge_single_run() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        let usage = serde_json::json!({
            "input_tokens": 1000_u64,
            "output_tokens": 500_u64,
            "cache_creation_input_tokens": 200_u64,
            "cache_read_input_tokens": 300_u64
        });
        let run = make_test_run(
            job_id,
            dt,
            Some(0.50),
            Some("claude-sonnet-4-20250514"),
            Some(usage),
        );
        manifest.merge_run(&run);

        assert_eq!(manifest.total_runs, 1);
        assert!((manifest.total_cost_usd - 0.50).abs() < f64::EPSILON);

        // Daily bucket
        let daily = manifest
            .daily_buckets
            .get("2025-06-15")
            .expect("daily bucket");
        assert_eq!(daily.runs, 1);
        assert!((daily.cost_usd - 0.50).abs() < f64::EPSILON);

        // Weekly bucket
        let weekly = manifest
            .weekly_buckets
            .get("2025-W24")
            .expect("weekly bucket");
        assert_eq!(weekly.runs, 1);

        // Monthly bucket
        let monthly = manifest
            .monthly_buckets
            .get("2025-06")
            .expect("monthly bucket");
        assert_eq!(monthly.runs, 1);

        // Model entry in daily bucket
        let model_entry = daily
            .models
            .get("claude-sonnet-4-20250514")
            .expect("model entry");
        assert_eq!(model_entry.input_tokens, 1000);
        assert_eq!(model_entry.output_tokens, 500);
        assert_eq!(model_entry.cache_creation_input_tokens, 200);
        assert_eq!(model_entry.cache_read_input_tokens, 300);
    }

    #[test]
    fn test_merge_multiple_runs_same_day() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt1 = Utc.with_ymd_and_hms(2025, 6, 15, 9, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2025, 6, 15, 14, 0, 0).unwrap();

        let run1 = make_test_run(
            job_id,
            dt1,
            Some(0.30),
            Some("claude-sonnet-4-20250514"),
            None,
        );
        let run2 = make_test_run(
            job_id,
            dt2,
            Some(0.20),
            Some("claude-sonnet-4-20250514"),
            None,
        );

        manifest.merge_run(&run1);
        manifest.merge_run(&run2);

        assert_eq!(manifest.total_runs, 2);
        assert!((manifest.total_cost_usd - 0.50).abs() < 1e-10);

        // Only one daily bucket
        assert_eq!(manifest.daily_buckets.len(), 1);
        let daily = manifest
            .daily_buckets
            .get("2025-06-15")
            .expect("daily bucket");
        assert_eq!(daily.runs, 2);
        assert!((daily.cost_usd - 0.50).abs() < 1e-10);

        // Only one weekly bucket
        assert_eq!(manifest.weekly_buckets.len(), 1);
        // Only one monthly bucket
        assert_eq!(manifest.monthly_buckets.len(), 1);
    }

    #[test]
    fn test_merge_multiple_runs_different_days() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        // 2025-06-15 → week 24, month 06
        let dt1 = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        // 2025-06-16 → week 25, month 06
        let dt2 = Utc.with_ymd_and_hms(2025, 6, 16, 10, 0, 0).unwrap();
        // 2025-06-20 → week 25, month 06
        let dt3 = Utc.with_ymd_and_hms(2025, 6, 20, 10, 0, 0).unwrap();

        manifest.merge_run(&make_test_run(job_id, dt1, Some(0.10), None, None));
        manifest.merge_run(&make_test_run(job_id, dt2, Some(0.20), None, None));
        manifest.merge_run(&make_test_run(job_id, dt3, Some(0.30), None, None));

        assert_eq!(manifest.total_runs, 3);

        // 3 separate daily buckets
        assert_eq!(manifest.daily_buckets.len(), 3);
        assert!(manifest.daily_buckets.contains_key("2025-06-15"));
        assert!(manifest.daily_buckets.contains_key("2025-06-16"));
        assert!(manifest.daily_buckets.contains_key("2025-06-20"));

        // 2 weekly buckets: W24 (only 6/15) and W25 (6/16 + 6/20)
        assert_eq!(manifest.weekly_buckets.len(), 2);
        let w24 = manifest.weekly_buckets.get("2025-W24").expect("week 24");
        assert_eq!(w24.runs, 1);
        let w25 = manifest.weekly_buckets.get("2025-W25").expect("week 25");
        assert_eq!(w25.runs, 2);

        // 1 monthly bucket: all in June
        assert_eq!(manifest.monthly_buckets.len(), 1);
        let june = manifest.monthly_buckets.get("2025-06").expect("june");
        assert_eq!(june.runs, 3);
    }

    #[test]
    fn test_merge_run_no_cost_data() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        let run = make_test_run(job_id, dt, None, None, None);
        manifest.merge_run(&run);

        assert_eq!(manifest.total_runs, 1);
        assert_eq!(manifest.total_cost_usd, 0.0);
        assert_eq!(manifest.total_duration_ms, 0);

        let daily = manifest
            .daily_buckets
            .get("2025-06-15")
            .expect("daily bucket");
        assert_eq!(daily.runs, 1);
        assert_eq!(daily.cost_usd, 0.0);
        assert!(daily.models.is_empty());
    }

    #[test]
    fn test_per_model_token_breakdown() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        let usage = serde_json::json!({
            "input_tokens": 1000_u64,
            "output_tokens": 500_u64,
            "cache_creation_input_tokens": 200_u64,
            "cache_read_input_tokens": 300_u64
        });
        let run = make_test_run(
            job_id,
            dt,
            Some(0.75),
            Some("claude-sonnet-4-20250514"),
            Some(usage),
        );
        manifest.merge_run(&run);

        // Check all three time bucket levels
        for bucket_map in [
            &manifest.daily_buckets,
            &manifest.weekly_buckets,
            &manifest.monthly_buckets,
        ] {
            let bucket = bucket_map.values().next().expect("bucket exists");
            let model_entry = bucket
                .models
                .get("claude-sonnet-4-20250514")
                .expect("model entry");
            assert_eq!(model_entry.runs, 1);
            assert!((model_entry.cost_usd - 0.75).abs() < f64::EPSILON);
            assert_eq!(model_entry.input_tokens, 1000);
            assert_eq!(model_entry.output_tokens, 500);
            assert_eq!(model_entry.cache_creation_input_tokens, 200);
            assert_eq!(model_entry.cache_read_input_tokens, 300);
        }
    }

    #[test]
    fn test_multiple_models() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();

        let usage1 = serde_json::json!({
            "input_tokens": 100_u64,
            "output_tokens": 50_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 0_u64
        });
        let run1 = make_test_run(
            job_id,
            dt,
            Some(0.10),
            Some("claude-sonnet-4-20250514"),
            Some(usage1),
        );

        let usage2 = serde_json::json!({
            "input_tokens": 200_u64,
            "output_tokens": 100_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 0_u64
        });
        let run2 = make_test_run(
            job_id,
            dt,
            Some(0.20),
            Some("claude-opus-4-5"),
            Some(usage2),
        );

        manifest.merge_run(&run1);
        manifest.merge_run(&run2);

        let daily = manifest
            .daily_buckets
            .get("2025-06-15")
            .expect("daily bucket");
        assert_eq!(daily.models.len(), 2);

        let sonnet = daily
            .models
            .get("claude-sonnet-4-20250514")
            .expect("sonnet entry");
        assert_eq!(sonnet.runs, 1);
        assert_eq!(sonnet.input_tokens, 100);
        assert_eq!(sonnet.output_tokens, 50);

        let opus = daily.models.get("claude-opus-4-5").expect("opus entry");
        assert_eq!(opus.runs, 1);
        assert_eq!(opus.input_tokens, 200);
        assert_eq!(opus.output_tokens, 100);
    }

    #[test]
    fn test_backward_compat_deserialize() {
        // A minimal manifest JSON missing newer optional fields should deserialize with defaults
        let json = r#"{
            "job_id": "01234567-8901-2345-6789-012345678901",
            "version": 1,
            "updated_at": "2025-06-15T10:00:00Z",
            "total_runs": 5,
            "total_cost_usd": 1.25
        }"#;

        let manifest: JobManifest = serde_json::from_str(json).expect("deserialize");
        assert_eq!(manifest.total_runs, 5);
        assert!((manifest.total_cost_usd - 1.25).abs() < f64::EPSILON);
        assert_eq!(manifest.total_duration_ms, 0);
        assert!(manifest.daily_buckets.is_empty());
        assert!(manifest.weekly_buckets.is_empty());
        assert!(manifest.monthly_buckets.is_empty());
    }

    #[test]
    fn test_runs_by_status() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();

        let run_completed = make_test_run_with_status(
            job_id,
            dt,
            Some(0.10),
            None,
            None,
            RunStatus::Completed,
        );
        let run_failed = make_test_run_with_status(
            job_id,
            dt,
            Some(0.05),
            None,
            None,
            RunStatus::Failed,
        );
        let run_completed_with_warnings = make_test_run_with_status(
            job_id,
            dt,
            Some(0.08),
            None,
            None,
            RunStatus::CompletedWithWarnings,
        );
        let run_killed = make_test_run_with_status(
            job_id,
            dt,
            Some(0.02),
            None,
            None,
            RunStatus::Killed,
        );

        manifest.merge_run(&run_completed);
        manifest.merge_run(&run_failed);
        manifest.merge_run(&run_completed_with_warnings);
        manifest.merge_run(&run_killed);

        let daily = manifest
            .daily_buckets
            .get("2025-06-15")
            .expect("daily bucket");

        assert_eq!(daily.runs, 4);
        assert_eq!(daily.runs_by_status.get("Completed"), Some(&1));
        assert_eq!(daily.runs_by_status.get("Failed"), Some(&1));
        assert_eq!(daily.runs_by_status.get("CompletedWithWarnings"), Some(&1));
        assert_eq!(daily.runs_by_status.get("Killed"), Some(&1));

        // Verify the same for weekly and monthly buckets
        let weekly = manifest
            .weekly_buckets
            .get("2025-W24")
            .expect("weekly bucket");
        assert_eq!(weekly.runs_by_status.get("Completed"), Some(&1));
        assert_eq!(weekly.runs_by_status.get("Failed"), Some(&1));
        assert_eq!(weekly.runs_by_status.get("CompletedWithWarnings"), Some(&1));
        assert_eq!(weekly.runs_by_status.get("Killed"), Some(&1));

        let monthly = manifest
            .monthly_buckets
            .get("2025-06")
            .expect("monthly bucket");
        assert_eq!(monthly.runs_by_status.get("Completed"), Some(&1));
        assert_eq!(monthly.runs_by_status.get("Failed"), Some(&1));
        assert_eq!(monthly.runs_by_status.get("CompletedWithWarnings"), Some(&1));
        assert_eq!(monthly.runs_by_status.get("Killed"), Some(&1));
    }
}
