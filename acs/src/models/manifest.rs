use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Deserializer, Serialize};
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

    pub fn summarize(
        &self,
        start: NaiveDate,
        end: NaiveDate,
    ) -> (CostSummaryData, Vec<DailyDataPoint>) {
        let mut total_runs: u64 = 0;
        let mut total_cost_usd: f64 = 0.0;
        let mut total_duration_ms: u64 = 0;
        let mut total_input_tokens: u64 = 0;
        let mut total_output_tokens: u64 = 0;
        let mut total_cache_read_tokens: u64 = 0;
        let mut runs_by_status: BTreeMap<String, u64> = BTreeMap::new();
        let mut data_points: Vec<DailyDataPoint> = Vec::new();

        for (key, bucket) in &self.daily_buckets {
            let date = match NaiveDate::parse_from_str(key, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => continue,
            };
            if date < start || date > end {
                continue;
            }

            total_runs += bucket.runs;
            total_cost_usd += bucket.cost_usd;
            total_duration_ms += bucket.duration_ms;

            for (status, count) in &bucket.runs_by_status {
                *runs_by_status.entry(status.clone()).or_insert(0) += count;
            }

            let mut day_input: u64 = 0;
            let mut day_output: u64 = 0;
            for model_bucket in bucket.models.values() {
                total_input_tokens += model_bucket.input_tokens;
                total_output_tokens += model_bucket.output_tokens;
                total_cache_read_tokens += model_bucket.cache_read_input_tokens;
                day_input += model_bucket.input_tokens;
                day_output += model_bucket.output_tokens;
            }

            data_points.push(DailyDataPoint {
                date: key.clone(),
                runs: bucket.runs,
                cost: bucket.cost_usd,
                input_tokens: day_input,
                output_tokens: day_output,
            });
        }

        data_points.sort_by(|a, b| a.date.cmp(&b.date));

        let avg_cost_per_run = if total_runs > 0 {
            total_cost_usd / total_runs as f64
        } else {
            0.0
        };

        let summary = CostSummaryData {
            total_runs,
            total_cost_usd,
            avg_cost_per_run,
            total_duration_ms,
            total_input_tokens,
            total_output_tokens,
            total_cache_read_tokens,
            runs_by_status,
        };

        (summary, data_points)
    }
}

pub fn resolve_timeframe(
    timeframe: &Timeframe,
    custom_start: Option<NaiveDate>,
    custom_end: Option<NaiveDate>,
) -> (NaiveDate, NaiveDate) {
    if let (Some(start), Some(end)) = (custom_start, custom_end) {
        return (start, end);
    }
    let end = Utc::now().date_naive();
    let start = match timeframe.to_days() {
        Some(days) => end - chrono::Duration::days(days - 1),
        None => NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
    };
    (start, end)
}

// ── Timeframe ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
pub enum Timeframe {
    Last24h,
    Last7d,
    #[default]
    Last30d,
    Last90d,
    Last180d,
    Last365d,
    All,
}

impl Timeframe {
    pub fn to_days(&self) -> Option<i64> {
        match self {
            Timeframe::Last24h => Some(1),
            Timeframe::Last7d => Some(7),
            Timeframe::Last30d => Some(30),
            Timeframe::Last90d => Some(90),
            Timeframe::Last180d => Some(180),
            Timeframe::Last365d => Some(365),
            Timeframe::All => None,
        }
    }
}

impl<'de> Deserialize<'de> for Timeframe {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        match s.as_str() {
            "24h" => Ok(Timeframe::Last24h),
            "7d" => Ok(Timeframe::Last7d),
            "30d" => Ok(Timeframe::Last30d),
            "90d" => Ok(Timeframe::Last90d),
            "180d" => Ok(Timeframe::Last180d),
            "365d" => Ok(Timeframe::Last365d),
            "all" => Ok(Timeframe::All),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["24h", "7d", "30d", "90d", "180d", "365d", "all"],
            )),
        }
    }
}

// ── Response structs ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CostSummaryData {
    pub total_runs: u64,
    pub total_cost_usd: f64,
    pub avg_cost_per_run: f64,
    pub total_duration_ms: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub runs_by_status: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DailyDataPoint {
    pub date: String,
    pub runs: u64,
    pub cost: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PerJobCostResponse {
    pub job_id: String,
    pub timeframe: String,
    pub summary: CostSummaryData,
    pub data: Vec<DailyDataPoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TopJobEntry {
    pub job_id: String,
    pub job_name: String,
    pub total_cost: f64,
    pub total_runs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct TodayTokens {
    pub input: u64,
    pub output: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct DailyTrendPoint {
    pub date: String,
    pub cost_usd: f64,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct GlobalCostResponse {
    pub timeframe: String,
    pub today_usd: f64,
    pub week_usd: f64,
    pub month_usd: f64,
    pub today_tokens: TodayTokens,
    pub top_jobs: Vec<TopJobEntry>,
    pub daily_trend: Vec<DailyTrendPoint>,
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

    // ── Timeframe deserialization tests ──────────────────────────────────────

    #[test]
    fn test_timeframe_deserialization() {
        let cases = [
            ("\"24h\"", Timeframe::Last24h),
            ("\"7d\"", Timeframe::Last7d),
            ("\"30d\"", Timeframe::Last30d),
            ("\"90d\"", Timeframe::Last90d),
            ("\"180d\"", Timeframe::Last180d),
            ("\"365d\"", Timeframe::Last365d),
            ("\"all\"", Timeframe::All),
        ];
        for (input, expected) in cases {
            let result: Timeframe = serde_json::from_str(input).expect(input);
            assert_eq!(result, expected, "failed for input: {}", input);
        }
    }

    #[test]
    fn test_timeframe_unknown_variant() {
        let result: Result<Timeframe, _> = serde_json::from_str("\"2d\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_timeframe_to_days() {
        assert_eq!(Timeframe::Last24h.to_days(), Some(1));
        assert_eq!(Timeframe::Last7d.to_days(), Some(7));
        assert_eq!(Timeframe::Last30d.to_days(), Some(30));
        assert_eq!(Timeframe::Last90d.to_days(), Some(90));
        assert_eq!(Timeframe::Last180d.to_days(), Some(180));
        assert_eq!(Timeframe::Last365d.to_days(), Some(365));
        assert_eq!(Timeframe::All.to_days(), None);
    }

    #[test]
    fn test_timeframe_default() {
        assert_eq!(Timeframe::default(), Timeframe::Last30d);
    }

    // ── summarize() tests ─────────────────────────────────────────────────────

    fn make_manifest_with_buckets() -> (Uuid, JobManifest) {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        // "recent" runs: 2025-06-14 and 2025-06-15 (within last 7 days of 2025-06-20)
        let dt_old = Utc.with_ymd_and_hms(2025, 1, 5, 10, 0, 0).unwrap();
        let dt_mid = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
        let dt_recent = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();

        let usage_old = serde_json::json!({
            "input_tokens": 1000_u64,
            "output_tokens": 200_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 50_u64
        });
        let usage_mid = serde_json::json!({
            "input_tokens": 300_u64,
            "output_tokens": 100_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 20_u64
        });
        let usage_recent = serde_json::json!({
            "input_tokens": 500_u64,
            "output_tokens": 150_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 30_u64
        });

        manifest.merge_run(&make_test_run(
            job_id,
            dt_old,
            Some(0.50),
            Some("model-a"),
            Some(usage_old),
        ));
        manifest.merge_run(&make_test_run(
            job_id,
            dt_mid,
            Some(0.20),
            Some("model-a"),
            Some(usage_mid),
        ));
        manifest.merge_run(&make_test_run(
            job_id,
            dt_recent,
            Some(0.30),
            Some("model-b"),
            Some(usage_recent),
        ));

        (job_id, manifest)
    }

    #[test]
    fn test_summarize_filter_last_7d() {
        let (_job_id, manifest) = make_manifest_with_buckets();

        let start = NaiveDate::from_ymd_opt(2025, 6, 9).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let (summary, data) = manifest.summarize(start, end);

        // Should include 2025-06-14 and 2025-06-15 only
        assert_eq!(summary.total_runs, 2);
        assert!((summary.total_cost_usd - 0.50).abs() < 1e-10);
        assert_eq!(data.len(), 2);
        assert_eq!(data[0].date, "2025-06-14");
        assert_eq!(data[1].date, "2025-06-15");
        assert_eq!(summary.total_input_tokens, 300 + 500);
        assert_eq!(summary.total_output_tokens, 100 + 150);
        assert_eq!(summary.total_cache_read_tokens, 20 + 30);
    }

    #[test]
    fn test_summarize_filter_all() {
        let (_job_id, manifest) = make_manifest_with_buckets();

        let start = NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2030, 12, 31).unwrap();
        let (summary, data) = manifest.summarize(start, end);

        assert_eq!(summary.total_runs, 3);
        assert!((summary.total_cost_usd - 1.00).abs() < 1e-10);
        assert_eq!(data.len(), 3);
        // Sorted by date ascending
        assert_eq!(data[0].date, "2025-01-05");
        assert_eq!(data[1].date, "2025-06-14");
        assert_eq!(data[2].date, "2025-06-15");
        assert_eq!(summary.total_input_tokens, 1000 + 300 + 500);
    }

    #[test]
    fn test_summarize_custom_range() {
        let (_job_id, manifest) = make_manifest_with_buckets();

        // Only 2025-06-14
        let start = NaiveDate::from_ymd_opt(2025, 6, 14).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 6, 14).unwrap();
        let (summary, data) = manifest.summarize(start, end);

        assert_eq!(summary.total_runs, 1);
        assert!((summary.total_cost_usd - 0.20).abs() < 1e-10);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].date, "2025-06-14");
        assert_eq!(summary.avg_cost_per_run, 0.20);
    }

    #[test]
    fn test_summarize_empty_manifest() {
        let job_id = Uuid::now_v7();
        let manifest = JobManifest::new(job_id);

        let start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 12, 31).unwrap();
        let (summary, data) = manifest.summarize(start, end);

        assert_eq!(summary.total_runs, 0);
        assert_eq!(summary.total_cost_usd, 0.0);
        assert_eq!(summary.avg_cost_per_run, 0.0);
        assert_eq!(summary.total_input_tokens, 0);
        assert_eq!(summary.total_output_tokens, 0);
        assert_eq!(summary.total_cache_read_tokens, 0);
        assert!(summary.runs_by_status.is_empty());
        assert!(data.is_empty());
    }

    #[test]
    fn test_summarize_token_aggregation_multiple_models() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();

        let usage_a = serde_json::json!({
            "input_tokens": 400_u64,
            "output_tokens": 100_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 10_u64
        });
        let usage_b = serde_json::json!({
            "input_tokens": 600_u64,
            "output_tokens": 200_u64,
            "cache_creation_input_tokens": 0_u64,
            "cache_read_input_tokens": 25_u64
        });

        manifest.merge_run(&make_test_run(
            job_id,
            dt,
            Some(0.10),
            Some("model-a"),
            Some(usage_a),
        ));
        manifest.merge_run(&make_test_run(
            job_id,
            dt,
            Some(0.15),
            Some("model-b"),
            Some(usage_b),
        ));

        let start = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let (summary, data) = manifest.summarize(start, end);

        assert_eq!(summary.total_input_tokens, 400 + 600);
        assert_eq!(summary.total_output_tokens, 100 + 200);
        assert_eq!(summary.total_cache_read_tokens, 10 + 25);
        assert_eq!(data.len(), 1);
        assert_eq!(data[0].input_tokens, 400 + 600);
        assert_eq!(data[0].output_tokens, 100 + 200);
    }

    #[test]
    fn test_summarize_avg_cost_per_run() {
        let job_id = Uuid::now_v7();
        let mut manifest = JobManifest::new(job_id);

        let dt1 = Utc.with_ymd_and_hms(2025, 6, 14, 10, 0, 0).unwrap();
        let dt2 = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();

        manifest.merge_run(&make_test_run(job_id, dt1, Some(0.30), None, None));
        manifest.merge_run(&make_test_run(job_id, dt2, Some(0.10), None, None));

        let start = NaiveDate::from_ymd_opt(2025, 6, 14).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        let (summary, _) = manifest.summarize(start, end);

        assert_eq!(summary.total_runs, 2);
        assert!((summary.total_cost_usd - 0.40).abs() < 1e-10);
        assert!((summary.avg_cost_per_run - 0.20).abs() < 1e-10);
    }

    #[test]
    fn test_resolve_timeframe_custom_dates() {
        use super::resolve_timeframe;
        let start = NaiveDate::from_ymd_opt(2025, 3, 1).unwrap();
        let end = NaiveDate::from_ymd_opt(2025, 3, 31).unwrap();
        let (s, e) = resolve_timeframe(&Timeframe::Last7d, Some(start), Some(end));
        assert_eq!(s, start);
        assert_eq!(e, end);
    }

    #[test]
    fn test_resolve_timeframe_computed() {
        use super::resolve_timeframe;
        let (start, end) = resolve_timeframe(&Timeframe::Last7d, None, None);
        let today = Utc::now().date_naive();
        assert_eq!(end, today);
        let expected_start = today - chrono::Duration::days(6);
        assert_eq!(start, expected_start);
    }

    #[test]
    fn test_resolve_timeframe_all() {
        use super::resolve_timeframe;
        let (start, end) = resolve_timeframe(&Timeframe::All, None, None);
        let today = Utc::now().date_naive();
        assert_eq!(end, today);
        assert_eq!(start, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    }
}
