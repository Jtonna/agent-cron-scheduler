//! In-memory cost summary cache for workflows.
//!
//! Entries are valid until the next midnight in `display_tz`. The cache is
//! eagerly invalidated via [`CostCache::invalidate_and_recompute`], which is
//! called by the event-bus subscriber in `daemon/mod.rs` whenever a run
//! transitions to a terminal status.
//!
//! In addition to per-workflow `CostSummary` totals, the cache also holds a
//! 365-day rolling array of `DailyBucket` entries per workflow and a
//! system-wide aggregate. Sub-window slices are served from the in-memory
//! arrays without hitting the store.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::{CostSummary, DailyBucket};
use crate::storage::workflow_runs::WorkflowRunStore;

// ─── Internal entries ─────────────────────────────────────────────────────────

#[derive(Clone)]
struct CachedEntry {
    summary: CostSummary,
    /// Wall-clock UTC timestamp of the next midnight in `display_tz`.
    /// The entry is stale once `Utc::now() >= valid_until`.
    valid_until: DateTime<Utc>,
}

/// Cached 365-day rolling array of daily buckets for a single workflow (or the
/// system aggregate). Buckets are in ascending date order.
#[derive(Clone)]
struct CachedDailyBuckets {
    /// Full 365-day window, ascending by date.
    full_buckets: Vec<DailyBucket>,
    /// Next midnight in display_tz — stale once `Utc::now() >= valid_until`.
    valid_until: DateTime<Utc>,
}

// ─── CostCache ────────────────────────────────────────────────────────────────

pub struct CostCache {
    /// Per-workflow CostSummary totals.
    inner: Arc<RwLock<HashMap<Uuid, CachedEntry>>>,
    /// Per-workflow 365-day daily-bucket arrays.
    daily_inner: Arc<RwLock<HashMap<Uuid, CachedDailyBuckets>>>,
    /// System-wide 365-day daily-bucket array.
    system_daily: Arc<RwLock<Option<CachedDailyBuckets>>>,
    run_store: Arc<dyn WorkflowRunStore>,
    display_tz: Tz,
}

impl CostCache {
    pub fn new(run_store: Arc<dyn WorkflowRunStore>, display_tz: Tz) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
            daily_inner: Arc::new(RwLock::new(HashMap::new())),
            system_daily: Arc::new(RwLock::new(None)),
            run_store,
            display_tz,
        }
    }

    /// Returns the cost summary for `workflow_id`, hitting the cache if the
    /// entry is still valid, or re-computing from the store if not.
    pub async fn get(&self, workflow_id: Uuid) -> Result<CostSummary, AcsError> {
        // Fast path: valid cache hit.
        {
            let guard = self.inner.read().await;
            if let Some(entry) = guard.get(&workflow_id) {
                if Utc::now() < entry.valid_until {
                    return Ok(entry.summary.clone());
                }
            }
        }

        // Slow path: compute and cache.
        let summary = self
            .run_store
            .cost_summary_for(workflow_id, &self.display_tz)
            .await?;
        let valid_until = next_midnight_in_tz(Utc::now(), self.display_tz);
        {
            let mut guard = self.inner.write().await;
            guard.insert(
                workflow_id,
                CachedEntry {
                    summary: summary.clone(),
                    valid_until,
                },
            );
        }
        Ok(summary)
    }

    /// Returns daily buckets for `workflow_id` filtered to `[since, until)`.
    ///
    /// Serves from the cached 365-day array if present and unexpired; otherwise
    /// fetches the full 365-day window from the store, caches it, then slices.
    pub async fn get_daily_buckets_for(
        &self,
        workflow_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<DailyBucket>, AcsError> {
        // Fast path: valid cache hit.
        {
            let guard = self.daily_inner.read().await;
            if let Some(entry) = guard.get(&workflow_id) {
                if Utc::now() < entry.valid_until {
                    return Ok(slice_buckets(
                        &entry.full_buckets,
                        since,
                        until,
                        self.display_tz,
                    ));
                }
            }
        }

        // Slow path: fetch full 365-day window and cache.
        let (year_since, year_until) = year_window_utc();
        let full_buckets = self
            .run_store
            .daily_buckets_for(Some(workflow_id), year_since, year_until, &self.display_tz)
            .await?;
        let valid_until = next_midnight_in_tz(Utc::now(), self.display_tz);
        {
            let mut guard = self.daily_inner.write().await;
            guard.insert(
                workflow_id,
                CachedDailyBuckets {
                    full_buckets: full_buckets.clone(),
                    valid_until,
                },
            );
        }
        Ok(slice_buckets(&full_buckets, since, until, self.display_tz))
    }

    /// Returns system-wide daily buckets filtered to `[since, until)`.
    ///
    /// Uses the cached system-aggregate 365-day array, refreshing if needed.
    pub async fn get_system_daily_buckets(
        &self,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> Result<Vec<DailyBucket>, AcsError> {
        // Fast path: valid system cache.
        {
            let guard = self.system_daily.read().await;
            if let Some(ref entry) = *guard {
                if Utc::now() < entry.valid_until {
                    return Ok(slice_buckets(
                        &entry.full_buckets,
                        since,
                        until,
                        self.display_tz,
                    ));
                }
            }
        }

        // Slow path: compute and cache.
        let full_buckets = self.fetch_and_cache_system_daily().await?;
        Ok(slice_buckets(&full_buckets, since, until, self.display_tz))
    }

    /// Force-recompute a workflow's summary and update the cache.
    /// Called by the event-bus invalidator task whenever a run completes.
    ///
    /// Also refreshes the workflow's 365-day daily-bucket array AND the
    /// system-wide daily-bucket array.
    pub async fn invalidate_and_recompute(&self, workflow_id: Uuid) -> Result<(), AcsError> {
        // Recompute the CostSummary totals.
        let summary = self
            .run_store
            .cost_summary_for(workflow_id, &self.display_tz)
            .await?;
        let valid_until = next_midnight_in_tz(Utc::now(), self.display_tz);
        {
            let mut guard = self.inner.write().await;
            guard.insert(
                workflow_id,
                CachedEntry {
                    summary,
                    valid_until,
                },
            );
        }

        // Recompute the workflow's 365-day daily bucket array.
        self.invalidate_and_recompute_daily(workflow_id).await?;

        Ok(())
    }

    /// Recompute a workflow's 365-day daily-bucket array AND the system array.
    pub async fn invalidate_and_recompute_daily(&self, workflow_id: Uuid) -> Result<(), AcsError> {
        let (year_since, year_until) = year_window_utc();
        let full_buckets = self
            .run_store
            .daily_buckets_for(Some(workflow_id), year_since, year_until, &self.display_tz)
            .await?;
        let valid_until = next_midnight_in_tz(Utc::now(), self.display_tz);
        {
            let mut guard = self.daily_inner.write().await;
            guard.insert(
                workflow_id,
                CachedDailyBuckets {
                    full_buckets,
                    valid_until,
                },
            );
        }

        // Also refresh the system aggregate.
        self.fetch_and_cache_system_daily().await?;

        Ok(())
    }

    /// Fetch the system-wide 365-day daily-bucket array, cache it, and return it.
    async fn fetch_and_cache_system_daily(&self) -> Result<Vec<DailyBucket>, AcsError> {
        let (year_since, year_until) = year_window_utc();
        let full_buckets = self
            .run_store
            .daily_buckets_for(None, year_since, year_until, &self.display_tz)
            .await?;
        let valid_until = next_midnight_in_tz(Utc::now(), self.display_tz);
        {
            let mut guard = self.system_daily.write().await;
            *guard = Some(CachedDailyBuckets {
                full_buckets: full_buckets.clone(),
                valid_until,
            });
        }
        Ok(full_buckets)
    }

    /// Remove a workflow's entry. Called when a workflow is deleted.
    pub async fn forget(&self, workflow_id: Uuid) {
        self.inner.write().await.remove(&workflow_id);
        self.daily_inner.write().await.remove(&workflow_id);
    }
}

/// Return the [since, until) UTC bounds for a 365-day rolling window ending now.
fn year_window_utc() -> (DateTime<Utc>, DateTime<Utc>) {
    let until = Utc::now();
    let since = until - chrono::Duration::days(365);
    (since, until)
}

/// Slice `buckets` (ascending by date) to entries whose local date falls in
/// `[since_date, until_date)` where `since_date` / `until_date` are the dates
/// of `since` / `until` in `tz`.
fn slice_buckets(
    buckets: &[DailyBucket],
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    tz: Tz,
) -> Vec<DailyBucket> {
    let since_date = since.with_timezone(&tz).date_naive();
    let until_date = until.with_timezone(&tz).date_naive();
    buckets
        .iter()
        .filter(|b| b.date >= since_date && b.date < until_date)
        .cloned()
        .collect()
}

/// Compute the next 00:00 in `display_tz` after `now` (returned as UTC).
///
/// If `now` is exactly at midnight in `display_tz`, the *next* midnight
/// (tomorrow) is returned — the current moment's summary stays valid for the
/// rest of today.
fn next_midnight_in_tz(now: DateTime<Utc>, tz: Tz) -> DateTime<Utc> {
    let now_local = now.with_timezone(&tz);
    let today_naive = now_local.date_naive();
    // Start of tomorrow in local calendar
    let tomorrow_naive = today_naive
        .succ_opt()
        .expect("date arithmetic should not overflow");
    #[allow(deprecated)]
    let midnight_naive = tomorrow_naive
        .and_hms_opt(0, 0, 0)
        .expect("00:00:00 is always valid");

    // Resolve the local time to UTC, handling DST gaps.
    tz.from_local_datetime(&midnight_naive)
        .earliest()
        .unwrap_or_else(|| {
            tz.from_local_datetime(&midnight_naive)
                .latest()
                .expect("at least one mapping must exist")
        })
        .with_timezone(&Utc)
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use async_trait::async_trait;
    use chrono::Utc;

    use chrono::NaiveDate;

    use crate::models::workflow::{DailyBucket, WorkflowRun};

    // ── Mock WorkflowRunStore ─────────────────────────────────────────────────

    /// A minimal mock that records how many times `cost_summary_for` was called
    /// and returns a canned `CostSummary`. Also tracks `daily_buckets_for` calls
    /// and returns a configurable list of `DailyBucket`s.
    struct MockRunStore {
        call_count: Arc<AtomicUsize>,
        daily_call_count: Arc<AtomicUsize>,
        return_summary: CostSummary,
        return_buckets: Vec<DailyBucket>,
    }

    impl MockRunStore {
        fn new(summary: CostSummary) -> (Arc<Self>, Arc<AtomicUsize>) {
            let counter = Arc::new(AtomicUsize::new(0));
            let store = Arc::new(Self {
                call_count: Arc::clone(&counter),
                daily_call_count: Arc::new(AtomicUsize::new(0)),
                return_summary: summary,
                return_buckets: Vec::new(),
            });
            (store, counter)
        }

        fn new_with_buckets(
            summary: CostSummary,
            buckets: Vec<DailyBucket>,
        ) -> (Arc<Self>, Arc<AtomicUsize>, Arc<AtomicUsize>) {
            let summary_counter = Arc::new(AtomicUsize::new(0));
            let daily_counter = Arc::new(AtomicUsize::new(0));
            let store = Arc::new(Self {
                call_count: Arc::clone(&summary_counter),
                daily_call_count: Arc::clone(&daily_counter),
                return_summary: summary,
                return_buckets: buckets,
            });
            (store, summary_counter, daily_counter)
        }
    }

    #[async_trait]
    impl WorkflowRunStore for MockRunStore {
        async fn create_run(&self, _run: WorkflowRun) -> Result<(), AcsError> {
            unimplemented!()
        }
        async fn update_run(&self, _run: &WorkflowRun) -> Result<(), AcsError> {
            unimplemented!()
        }
        async fn get_run(&self, _run_id: Uuid) -> Result<Option<WorkflowRun>, AcsError> {
            unimplemented!()
        }
        async fn list_runs(
            &self,
            _workflow_id: Uuid,
            _limit: usize,
            _offset: usize,
        ) -> Result<Vec<WorkflowRun>, AcsError> {
            unimplemented!()
        }
        async fn count_runs(&self, _workflow_id: Uuid) -> Result<usize, AcsError> {
            unimplemented!()
        }
        async fn list_recent_runs(
            &self,
            _limit: usize,
            _offset: usize,
        ) -> Result<Vec<WorkflowRun>, AcsError> {
            unimplemented!()
        }
        async fn count_all_runs(&self) -> Result<usize, AcsError> {
            unimplemented!()
        }
        async fn delete_run(&self, _run_id: Uuid) -> Result<(), AcsError> {
            unimplemented!()
        }
        async fn purge_runs(&self, _workflow_id: Uuid) -> Result<Vec<Uuid>, AcsError> {
            unimplemented!()
        }
        async fn list_ledger_workflows(&self) -> Result<Vec<(Uuid, String)>, AcsError> {
            Ok(Vec::new())
        }
        async fn cost_summary_for(
            &self,
            _workflow_id: Uuid,
            _display_tz: &Tz,
        ) -> Result<CostSummary, AcsError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.return_summary.clone())
        }
        async fn daily_buckets_for(
            &self,
            _workflow_id: Option<Uuid>,
            _since: chrono::DateTime<Utc>,
            _until: chrono::DateTime<Utc>,
            _display_tz: &Tz,
        ) -> Result<Vec<DailyBucket>, AcsError> {
            self.daily_call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.return_buckets.clone())
        }
    }

    fn make_summary() -> CostSummary {
        CostSummary {
            last_30_days_total_usd: 1.23,
            last_30_days_runs: 5,
            last_year_total_usd: 10.0,
            last_year_runs: 42,
            computed_at: Utc::now(),
            daily_buckets: Vec::new(),
            last_30_days_input_tokens: 0,
            last_30_days_output_tokens: 0,
            last_year_input_tokens: 0,
            last_year_output_tokens: 0,
            last_30_days_avg_cost_per_run_usd: 1.23 / 5.0,
            last_year_avg_cost_per_run_usd: 10.0 / 42.0,
        }
    }

    fn make_bucket(date: NaiveDate, cost: f64) -> DailyBucket {
        DailyBucket {
            date,
            total_usd: cost,
            cost_from_completed: cost,
            cost_from_failed: 0.0,
            cost_from_killed: 0.0,
            runs_completed: 1,
            runs_failed: 0,
            runs_killed: 0,
            tokens_in_from_completed: 0,
            tokens_in_from_failed: 0,
            tokens_in_from_killed: 0,
            tokens_out_from_completed: 0,
            tokens_out_from_failed: 0,
            tokens_out_from_killed: 0,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    // 1. Cold cache → computes via store, stores in cache, returns.
    #[tokio::test]
    async fn test_get_cold_cache_calls_store() {
        let summary = make_summary();
        let (mock, counter) = MockRunStore::new(summary.clone());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);

        let result = cache.get(Uuid::now_v7()).await.expect("get");
        assert_eq!(result.last_30_days_runs, summary.last_30_days_runs);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // 2. Warm cache (within valid_until) → returns from cache, does NOT call store again.
    #[tokio::test]
    async fn test_get_warm_cache_does_not_call_store() {
        let summary = make_summary();
        let (mock, counter) = MockRunStore::new(summary.clone());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);

        let wf_id = Uuid::now_v7();
        let _ = cache.get(wf_id).await.expect("first get");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        let _ = cache.get(wf_id).await.expect("second get");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "warm cache should not call store again"
        );
    }

    // 3. Get after valid_until has passed → recomputes.
    #[tokio::test]
    async fn test_get_expired_entry_recomputes() {
        let summary = make_summary();
        let (mock, counter) = MockRunStore::new(summary.clone());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        // Manually insert an expired entry.
        {
            let mut guard = cache.inner.write().await;
            guard.insert(
                wf_id,
                CachedEntry {
                    summary: summary.clone(),
                    valid_until: Utc::now() - chrono::Duration::seconds(1),
                },
            );
        }

        let _ = cache.get(wf_id).await.expect("get after expiry");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "expired entry should trigger a store call"
        );
    }

    // 4. invalidate_and_recompute → updates the cache even if the entry is fresh.
    #[tokio::test]
    async fn test_invalidate_and_recompute_updates_entry() {
        let summary = make_summary();
        let (mock, counter) = MockRunStore::new(summary.clone());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        // Prime the cache.
        let _ = cache.get(wf_id).await.expect("get");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Invalidate — should call store again even though entry is fresh.
        cache
            .invalidate_and_recompute(wf_id)
            .await
            .expect("invalidate");
        assert_eq!(counter.load(Ordering::SeqCst), 2);

        // Subsequent get should be served from the newly cached value.
        let _ = cache.get(wf_id).await.expect("get after invalidate");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "should use new cache entry"
        );
    }

    // 5. forget → removes entry; next get recomputes.
    #[tokio::test]
    async fn test_forget_removes_entry() {
        let summary = make_summary();
        let (mock, counter) = MockRunStore::new(summary.clone());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        let _ = cache.get(wf_id).await.expect("prime");
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        cache.forget(wf_id).await;

        let _ = cache.get(wf_id).await.expect("get after forget");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            2,
            "forget should cause recompute on next get"
        );
    }

    // ── next_midnight_in_tz helper ────────────────────────────────────────────

    #[test]
    fn test_next_midnight_in_tz_utc() {
        // Given a time at noon UTC, next midnight should be 00:00 tomorrow UTC.
        let tz: Tz = "UTC".parse().unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-10T12:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let midnight = next_midnight_in_tz(now, tz);
        assert_eq!(
            midnight.to_rfc3339(),
            "2026-05-11T00:00:00+00:00",
            "next midnight in UTC should be 2026-05-11T00:00:00Z"
        );
    }

    #[test]
    fn test_next_midnight_in_tz_la_pdt() {
        // In PDT (UTC-7), midnight on May 11 = 07:00 UTC May 11.
        let tz: Tz = "America/Los_Angeles".parse().unwrap();
        let now = chrono::DateTime::parse_from_rfc3339("2026-05-10T12:00:00+00:00")
            .unwrap()
            .with_timezone(&Utc);
        let midnight = next_midnight_in_tz(now, tz);
        // PDT is UTC-7, so midnight local = 07:00 UTC.
        assert_eq!(
            midnight.to_rfc3339(),
            "2026-05-11T07:00:00+00:00",
            "next midnight in LA PDT should be 07:00 UTC"
        );
    }

    // ── Daily bucket cache tests ──────────────────────────────────────────────

    // 1. Cold cache: get_daily_buckets_for triggers store call.
    #[tokio::test]
    async fn test_daily_buckets_cache_cold_miss() {
        let buckets = vec![make_bucket(
            NaiveDate::from_ymd_opt(2026, 5, 9).unwrap(),
            1.0,
        )];
        let (mock, _summary_counter, daily_counter) =
            MockRunStore::new_with_buckets(make_summary(), buckets.clone());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let result = cache
            .get_daily_buckets_for(wf_id, since, until)
            .await
            .expect("get_daily_buckets_for");

        assert_eq!(
            daily_counter.load(Ordering::SeqCst),
            1,
            "cold miss must call the store"
        );
        assert_eq!(result.len(), 1);
    }

    // 2. Warm cache: second call within valid_until does NOT hit the store.
    #[tokio::test]
    async fn test_daily_buckets_cache_hit_slices() {
        // Build an array with entries on two dates.
        let date_in = NaiveDate::from_ymd_opt(2026, 5, 9).unwrap();
        let date_out = NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(); // outside the narrow slice

        let full_buckets = vec![make_bucket(date_out, 99.0), make_bucket(date_in, 1.0)];

        let (mock, _summary_counter, daily_counter) =
            MockRunStore::new_with_buckets(make_summary(), full_buckets);
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        // First call: cache miss — populates the array.
        let since_all: DateTime<Utc> = "2025-01-01T00:00:00+00:00".parse().unwrap();
        let until_all: DateTime<Utc> = "2027-01-01T00:00:00+00:00".parse().unwrap();
        let _ = cache
            .get_daily_buckets_for(wf_id, since_all, until_all)
            .await
            .expect("first call");
        assert_eq!(
            daily_counter.load(Ordering::SeqCst),
            1,
            "first call hits store"
        );

        // Second call: cache warm — should serve from in-memory, narrow slice returns only date_in.
        let since_narrow: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until_narrow: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let result = cache
            .get_daily_buckets_for(wf_id, since_narrow, until_narrow)
            .await
            .expect("second call");
        assert_eq!(
            daily_counter.load(Ordering::SeqCst),
            1,
            "warm cache must NOT call store again"
        );
        assert_eq!(
            result.len(),
            1,
            "slice should return only the in-window bucket"
        );
        assert_eq!(result[0].date, date_in);
    }

    // 3. invalidate_and_recompute refreshes both the workflow's and the system's daily arrays.
    #[tokio::test]
    async fn test_daily_buckets_eager_invalidation_refreshes_both_workflow_and_system() {
        let (mock, summary_counter, daily_counter) =
            MockRunStore::new_with_buckets(make_summary(), Vec::new());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        cache
            .invalidate_and_recompute(wf_id)
            .await
            .expect("invalidate_and_recompute");

        // Should have called cost_summary_for once.
        assert_eq!(
            summary_counter.load(Ordering::SeqCst),
            1,
            "cost_summary_for called once"
        );
        // Should have called daily_buckets_for twice:
        //   once for the workflow-specific array, once for the system aggregate.
        assert_eq!(
            daily_counter.load(Ordering::SeqCst),
            2,
            "daily_buckets_for called twice (workflow + system)"
        );
    }

    // 4. Expired daily entry triggers recompute on next get_daily_buckets_for.
    #[tokio::test]
    async fn test_daily_buckets_cache_expiry_at_midnight() {
        let (mock, _summary_counter, daily_counter) =
            MockRunStore::new_with_buckets(make_summary(), Vec::new());
        let tz: Tz = "UTC".parse().unwrap();
        let cache = CostCache::new(mock as Arc<dyn WorkflowRunStore>, tz);
        let wf_id = Uuid::now_v7();

        // Manually insert an already-expired daily cache entry.
        {
            let mut guard = cache.daily_inner.write().await;
            guard.insert(
                wf_id,
                CachedDailyBuckets {
                    full_buckets: Vec::new(),
                    valid_until: Utc::now() - chrono::Duration::seconds(1),
                },
            );
        }

        let since: DateTime<Utc> = "2026-05-01T00:00:00+00:00".parse().unwrap();
        let until: DateTime<Utc> = "2026-06-01T00:00:00+00:00".parse().unwrap();
        let _ = cache
            .get_daily_buckets_for(wf_id, since, until)
            .await
            .expect("get after expiry");

        assert_eq!(
            daily_counter.load(Ordering::SeqCst),
            1,
            "expired entry should trigger a store call"
        );
    }
}
