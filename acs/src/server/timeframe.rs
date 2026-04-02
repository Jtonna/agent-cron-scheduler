use std::collections::{BTreeMap, HashMap};

use chrono::{Local, NaiveDate};
use serde::{Deserialize, Serialize};

use crate::models::manifest::TimeBucket;

#[derive(Debug, Deserialize, Default)]
pub struct TimeframeQuery {
    pub timeframe: Option<String>,
    pub start: Option<String>,
    pub end: Option<String>,
}

pub fn resolve_date_range(query: &TimeframeQuery) -> Result<(NaiveDate, NaiveDate), String> {
    let has_timeframe = query.timeframe.is_some();
    let has_custom = query.start.is_some() || query.end.is_some();
    if has_timeframe && has_custom {
        return Err("Cannot combine timeframe with start/end -- use one or the other".to_string());
    }
    let today = Local::now().date_naive();
    if has_timeframe {
        let tf = query.timeframe.as_deref().unwrap();
        let days: i64 = match tf {
            "24h" => 1,
            "7d" => 7,
            "30d" => 30,
            "90d" => 90,
            "180d" => 180,
            "365d" => 365,
            "all" => {
                let start = NaiveDate::from_ymd_opt(2000, 1, 1).expect("static");
                return Ok((start, today));
            }
            other => return Err(format!("Invalid timeframe: {}. Valid: 24h, 7d, 30d, 90d, 180d, 365d, all", other)),
        };
        let start = today - chrono::Duration::days(days - 1);
        return Ok((start, today));
    }
    if has_custom {
        let start = if let Some(s) = &query.start {
            NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|e| format!("Invalid start date {} ({})", s, e))?
        } else {
            NaiveDate::from_ymd_opt(2000, 1, 1).expect("static")
        };
        let end = if let Some(e) = &query.end {
            NaiveDate::parse_from_str(e, "%Y-%m-%d")
                .map_err(|err| format!("Invalid end date {} ({})", e, err))?
        } else {
            today
        };
        if start > end {
            return Err(format!("start must be on or before end: {} > {}", start, end));
        }
        return Ok((start, end));
    }
    let start = today - chrono::Duration::days(29);
    Ok((start, today))
}

pub fn format_timeframe_label(start: NaiveDate, end: NaiveDate) -> String {
    let today = Local::now().date_naive();
    if end == today {
        let days = (today - start).num_days() + 1;
        match days {
            1 => return "24h".to_string(),
            7 => return "7d".to_string(),
            30 => return "30d".to_string(),
            90 => return "90d".to_string(),
            180 => return "180d".to_string(),
            365 => return "365d".to_string(),
            _ => {}
        }
        if start == NaiveDate::from_ymd_opt(2000, 1, 1).unwrap() {
            return "all".to_string();
        }
    }
    format!("{}/{}", start, end)
}

pub fn filter_daily_buckets(
    buckets: &BTreeMap<String, TimeBucket>,
    start: NaiveDate,
    end: NaiveDate,
) -> BTreeMap<String, TimeBucket> {
    let start_key = start.format("%Y-%m-%d").to_string();
    let end_key = end.format("%Y-%m-%d").to_string();
    buckets.range(start_key..=end_key).map(|(k, v)| (k.clone(), v.clone())).collect()
}

pub fn aggregate_daily_buckets(
    buckets: &BTreeMap<String, TimeBucket>,
) -> (CostSummaryStats, Vec<DailyDataPoint>) {
    let mut total_runs: u64 = 0;
    let mut total_cost_usd: f64 = 0.0;
    let mut total_duration_ms: u64 = 0;
    let mut total_input_tokens: u64 = 0;
    let mut total_output_tokens: u64 = 0;
    let mut total_cache_read_tokens: u64 = 0;
    let mut daily_points: Vec<DailyDataPoint> = Vec::with_capacity(buckets.len());
    for (date_key, bucket) in buckets {
        total_runs += bucket.runs;
        total_cost_usd += bucket.cost_usd;
        total_duration_ms += bucket.duration_ms;
        let mut day_input: u64 = 0;
        let mut day_output: u64 = 0;
        for model in bucket.models.values() {
            day_input += model.input_tokens;
            day_output += model.output_tokens;
            total_cache_read_tokens += model.cache_read_input_tokens;
        }
        total_input_tokens += day_input;
        total_output_tokens += day_output;
        daily_points.push(DailyDataPoint {
            date: date_key.clone(), runs: bucket.runs, cost: bucket.cost_usd,
            input_tokens: day_input, output_tokens: day_output,
        });
    }
    let avg = if total_runs == 0 { 0.0 } else { total_cost_usd / total_runs as f64 };
    (CostSummaryStats {
        total_runs, total_cost_usd, avg_cost_per_run: avg, total_duration_ms,
        total_input_tokens, total_output_tokens, total_cache_read_tokens,
        runs_by_status: HashMap::new(),
    }, daily_points)
}

pub fn merge_daily_buckets(
    a: &BTreeMap<String, TimeBucket>,
    b: &BTreeMap<String, TimeBucket>,
) -> BTreeMap<String, TimeBucket> {
    let mut result: BTreeMap<String, TimeBucket> = a.clone();
    for (key, bb) in b {
        let e = result.entry(key.clone()).or_default();
        e.runs += bb.runs;
        e.cost_usd += bb.cost_usd;
        e.duration_ms += bb.duration_ms;
        e.num_turns += bb.num_turns;
        for (mk, bm) in &bb.models {
            let m = e.models.entry(mk.clone()).or_default();
            m.runs += bm.runs;
            m.cost_usd += bm.cost_usd;
            m.input_tokens += bm.input_tokens;
            m.output_tokens += bm.output_tokens;
            m.cache_creation_input_tokens += bm.cache_creation_input_tokens;
            m.cache_read_input_tokens += bm.cache_read_input_tokens;
        }
        for (sk, cnt) in &bb.runs_by_status {
            *e.runs_by_status.entry(sk.clone()).or_insert(0) += cnt;
        }
    }
    result
}

#[derive(Debug, Serialize)]
pub struct CostSummaryResponse {
    pub job_id: String, pub timeframe: String,
    pub summary: CostSummaryStats, pub data: Vec<DailyDataPoint>,
}

#[derive(Debug, Serialize)]
pub struct CostSummaryStats {
    pub total_runs: u64, pub total_cost_usd: f64, pub avg_cost_per_run: f64,
    pub total_duration_ms: u64, pub total_input_tokens: u64,
    pub total_output_tokens: u64, pub total_cache_read_tokens: u64,
    pub runs_by_status: HashMap<String, u64>,
}

#[derive(Debug, Serialize)]
pub struct DailyDataPoint {
    pub date: String, pub runs: u64, pub cost: f64,
    pub input_tokens: u64, pub output_tokens: u64,
}

#[derive(Debug, Serialize)]
pub struct GlobalCostSummaryResponse {
    pub timeframe: String, pub today_usd: f64, pub week_usd: f64, pub month_usd: f64,
    pub today_tokens: TokenSummary, pub top_jobs: Vec<TopJobEntry>,
    pub daily_trend: Vec<DailyDataPoint>,
}

#[derive(Debug, Serialize)]
pub struct TokenSummary { pub input: u64, pub output: u64 }

#[derive(Debug, Serialize)]
pub struct TopJobEntry {
    pub job_id: String, pub job_name: String,
    pub total_cost: f64, pub total_runs: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::manifest::{ModelUsageBucket, TimeBucket};

    fn mb(runs: u64, cost: f64, dur: u64) -> TimeBucket {
        TimeBucket { runs, cost_usd: cost, duration_ms: dur, ..Default::default() }
    }
    fn mbm(runs: u64, cost: f64, model: &str, inp: u64, out: u64, cr: u64) -> TimeBucket {
        let mut models = BTreeMap::new();
        models.insert(model.to_string(), ModelUsageBucket {
            runs, cost_usd: cost, input_tokens: inp, output_tokens: out,
            cache_creation_input_tokens: 0, cache_read_input_tokens: cr,
        });
        TimeBucket { runs, cost_usd: cost, duration_ms: 1000, num_turns: 1, models, ..Default::default() }
    }

    #[test]
    fn test_default_is_30d() {
        let (start, end) = resolve_date_range(&TimeframeQuery::default()).unwrap();
        assert_eq!(end, Local::now().date_naive());
        assert_eq!((end - start).num_days(), 29);
    }

    #[test]
    fn test_preset_timeframes() {
        let today = Local::now().date_naive();
        for (tf, span) in [("24h", 0i64), ("7d", 6), ("30d", 29), ("90d", 89), ("180d", 179), ("365d", 364)] {
            let q = TimeframeQuery { timeframe: Some(tf.to_string()), ..Default::default() };
            let (start, end) = resolve_date_range(&q).unwrap();
            assert_eq!(end, today);
            assert_eq!((end - start).num_days(), span, "span for {}", tf);
        }
    }

    #[test]
    fn test_all_timeframe() {
        let q = TimeframeQuery { timeframe: Some("all".to_string()), ..Default::default() };
        let (start, end) = resolve_date_range(&q).unwrap();
        assert_eq!(end, Local::now().date_naive());
        assert_eq!(start, NaiveDate::from_ymd_opt(2000, 1, 1).unwrap());
    }

    #[test]
    fn test_invalid_timeframe_returns_error() {
        let q = TimeframeQuery { timeframe: Some("2w".to_string()), ..Default::default() };
        assert!(resolve_date_range(&q).is_err());
    }

    #[test]
    fn test_custom_start_and_end() {
        let q = TimeframeQuery { start: Some("2025-01-01".to_string()), end: Some("2025-01-31".to_string()), ..Default::default() };
        let (s, e) = resolve_date_range(&q).unwrap();
        assert_eq!(s, NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        assert_eq!(e, NaiveDate::from_ymd_opt(2025, 1, 31).unwrap());
    }

    #[test]
    fn test_custom_start_only_end_defaults_to_today() {
        let q = TimeframeQuery { start: Some("2025-06-01".to_string()), ..Default::default() };
        let (s, e) = resolve_date_range(&q).unwrap();
        assert_eq!(s, NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
        assert_eq!(e, Local::now().date_naive());
    }

    #[test]
    fn test_conflict_timeframe_and_start_returns_error() {
        let q = TimeframeQuery { timeframe: Some("7d".to_string()), start: Some("2025-01-01".to_string()), ..Default::default() };
        assert!(resolve_date_range(&q).is_err());
    }

    #[test]
    fn test_start_after_end_returns_error() {
        let q = TimeframeQuery { start: Some("2025-06-30".to_string()), end: Some("2025-06-01".to_string()), ..Default::default() };
        assert!(resolve_date_range(&q).is_err());
    }

    #[test]
    fn test_invalid_date_format_returns_error() {
        let q = TimeframeQuery { start: Some("01-06-2025".to_string()), ..Default::default() };
        assert!(resolve_date_range(&q).is_err());
    }

    #[test]
    fn test_filter_inclusive_range() {
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mb(1, 0.1, 100));
        b.insert("2025-06-15".to_string(), mb(2, 0.2, 200));
        b.insert("2025-06-20".to_string(), mb(3, 0.3, 300));
        b.insert("2025-06-25".to_string(), mb(4, 0.4, 400));
        let f = filter_daily_buckets(&b, NaiveDate::from_ymd_opt(2025,6,15).unwrap(), NaiveDate::from_ymd_opt(2025,6,20).unwrap());
        assert_eq!(f.len(), 2);
        assert!(f.contains_key("2025-06-15"));
        assert!(f.contains_key("2025-06-20"));
        assert!(!f.contains_key("2025-06-10"));
        assert!(!f.contains_key("2025-06-25"));
    }

    #[test]
    fn test_filter_empty_result() {
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mb(1, 0.1, 100));
        assert!(filter_daily_buckets(&b, NaiveDate::from_ymd_opt(2025,7,1).unwrap(), NaiveDate::from_ymd_opt(2025,7,31).unwrap()).is_empty());
    }

    #[test]
    fn test_filter_boundary_dates_inclusive() {
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-01".to_string(), mb(1, 0.1, 100));
        b.insert("2025-06-30".to_string(), mb(2, 0.2, 200));
        assert_eq!(filter_daily_buckets(&b, NaiveDate::from_ymd_opt(2025,6,1).unwrap(), NaiveDate::from_ymd_opt(2025,6,30).unwrap()).len(), 2);
    }

    #[test]
    fn test_aggregate_empty_buckets() {
        let (stats, pts) = aggregate_daily_buckets(&BTreeMap::new());
        assert_eq!(stats.total_runs, 0);
        assert_eq!(stats.avg_cost_per_run, 0.0);
        assert!(pts.is_empty());
    }

    #[test]
    fn test_aggregate_sums_correctly() {
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mbm(2, 0.20, "ma", 100, 50, 10));
        b.insert("2025-06-11".to_string(), mbm(3, 0.30, "ma", 200, 100, 20));
        let (stats, pts) = aggregate_daily_buckets(&b);
        assert_eq!(stats.total_runs, 5);
        assert!((stats.total_cost_usd - 0.50).abs() < 1e-10);
        assert!((stats.avg_cost_per_run - 0.10).abs() < 1e-10);
        assert_eq!(stats.total_input_tokens, 300);
        assert_eq!(stats.total_output_tokens, 150);
        assert_eq!(stats.total_cache_read_tokens, 30);
        assert_eq!(pts.len(), 2);
        assert_eq!(pts[0].date, "2025-06-10");
        assert_eq!(pts[1].date, "2025-06-11");
    }

    #[test]
    fn test_aggregate_multi_model_tokens() {
        let mut models = BTreeMap::new();
        models.insert("ma".to_string(), ModelUsageBucket { runs: 1, cost_usd: 0.10, input_tokens: 100, output_tokens: 50, cache_creation_input_tokens: 0, cache_read_input_tokens: 5 });
        models.insert("mb_model".to_string(), ModelUsageBucket { runs: 1, cost_usd: 0.20, input_tokens: 200, output_tokens: 100, cache_creation_input_tokens: 0, cache_read_input_tokens: 10 });
        let bucket = TimeBucket { runs: 2, cost_usd: 0.30, duration_ms: 2000, num_turns: 2, models, ..Default::default() };
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-15".to_string(), bucket);
        let (stats, pts) = aggregate_daily_buckets(&b);
        assert_eq!(stats.total_input_tokens, 300);
        assert_eq!(stats.total_output_tokens, 150);
        assert_eq!(stats.total_cache_read_tokens, 15);
        assert_eq!(pts[0].input_tokens, 300);
        assert_eq!(pts[0].output_tokens, 150);
    }

    #[test]
    fn test_merge_non_overlapping() {
        let mut a: BTreeMap<String, TimeBucket> = BTreeMap::new();
        a.insert("2025-06-10".to_string(), mb(1, 0.10, 100));
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-11".to_string(), mb(2, 0.20, 200));
        let m = merge_daily_buckets(&a, &b);
        assert_eq!(m.len(), 2);
        assert_eq!(m["2025-06-10"].runs, 1);
        assert_eq!(m["2025-06-11"].runs, 2);
    }

    #[test]
    fn test_merge_overlapping_sums_fields() {
        let mut a: BTreeMap<String, TimeBucket> = BTreeMap::new();
        a.insert("2025-06-10".to_string(), mb(1, 0.10, 100));
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mb(2, 0.20, 200));
        let m = merge_daily_buckets(&a, &b);
        assert_eq!(m.len(), 1);
        assert_eq!(m["2025-06-10"].runs, 3);
        assert!((m["2025-06-10"].cost_usd - 0.30).abs() < 1e-10);
        assert_eq!(m["2025-06-10"].duration_ms, 300);
    }

    #[test]
    fn test_merge_model_maps_combined() {
        let mut a: BTreeMap<String, TimeBucket> = BTreeMap::new();
        a.insert("2025-06-10".to_string(), mbm(1, 0.10, "ma", 100, 50, 0));
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mbm(1, 0.20, "mb_model", 200, 100, 0));
        let m = merge_daily_buckets(&a, &b);
        assert_eq!(m["2025-06-10"].models.len(), 2);
        assert!(m["2025-06-10"].models.contains_key("ma"));
        assert!(m["2025-06-10"].models.contains_key("mb_model"));
    }

    #[test]
    fn test_merge_same_model_sums_tokens() {
        let mut a: BTreeMap<String, TimeBucket> = BTreeMap::new();
        a.insert("2025-06-10".to_string(), mbm(1, 0.10, "ma", 100, 50, 5));
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mbm(2, 0.20, "ma", 200, 100, 10));
        let m = merge_daily_buckets(&a, &b);
        let model = m["2025-06-10"].models.get("ma").unwrap();
        assert_eq!(model.runs, 3);
        assert_eq!(model.input_tokens, 300);
        assert_eq!(model.output_tokens, 150);
        assert_eq!(model.cache_read_input_tokens, 15);
    }

    #[test]
    fn test_merge_empty_maps() {
        let a: BTreeMap<String, TimeBucket> = BTreeMap::new();
        let mut b: BTreeMap<String, TimeBucket> = BTreeMap::new();
        b.insert("2025-06-10".to_string(), mb(1, 0.10, 100));
        assert_eq!(merge_daily_buckets(&a, &b).len(), 1);
        assert_eq!(merge_daily_buckets(&b, &a).len(), 1);
    }
}
