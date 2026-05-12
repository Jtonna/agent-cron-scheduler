//! In-memory cost summary cache for workflows.
//!
//! Entries are valid until the next midnight in `display_tz`. The cache is
//! eagerly invalidated via [`CostCache::invalidate_and_recompute`], which is
//! called by the event-bus subscriber in `daemon/mod.rs` whenever a run
//! transitions to a terminal status.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::errors::AcsError;
use crate::models::workflow::CostSummary;
use crate::storage::workflow_runs::WorkflowRunStore;

// ─── Internal entry ───────────────────────────────────────────────────────────

#[derive(Clone)]
struct CachedEntry {
    summary: CostSummary,
    /// Wall-clock UTC timestamp of the next midnight in `display_tz`.
    /// The entry is stale once `Utc::now() >= valid_until`.
    valid_until: DateTime<Utc>,
}

// ─── CostCache ────────────────────────────────────────────────────────────────

pub struct CostCache {
    inner: Arc<RwLock<HashMap<Uuid, CachedEntry>>>,
    run_store: Arc<dyn WorkflowRunStore>,
    display_tz: Tz,
}

impl CostCache {
    pub fn new(run_store: Arc<dyn WorkflowRunStore>, display_tz: Tz) -> Self {
        Self {
            inner: Arc::new(RwLock::new(HashMap::new())),
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

    /// Force-recompute a workflow's summary and update the cache.
    /// Called by the event-bus invalidator task whenever a run completes.
    pub async fn invalidate_and_recompute(&self, workflow_id: Uuid) -> Result<(), AcsError> {
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
        Ok(())
    }

    /// Remove a workflow's entry. Called when a workflow is deleted.
    pub async fn forget(&self, workflow_id: Uuid) {
        self.inner.write().await.remove(&workflow_id);
    }
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

    use crate::models::workflow::WorkflowRun;

    // ── Mock WorkflowRunStore ─────────────────────────────────────────────────

    /// A minimal mock that records how many times `cost_summary_for` was called
    /// and returns a canned `CostSummary`.
    struct MockRunStore {
        call_count: Arc<AtomicUsize>,
        return_summary: CostSummary,
    }

    impl MockRunStore {
        fn new(summary: CostSummary) -> (Arc<Self>, Arc<AtomicUsize>) {
            let counter = Arc::new(AtomicUsize::new(0));
            let store = Arc::new(Self {
                call_count: Arc::clone(&counter),
                return_summary: summary,
            });
            (store, counter)
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
        async fn delete_run(&self, _run_id: Uuid) -> Result<(), AcsError> {
            unimplemented!()
        }
        async fn cost_summary_for(
            &self,
            _workflow_id: Uuid,
            _display_tz: &Tz,
        ) -> Result<CostSummary, AcsError> {
            self.call_count.fetch_add(1, Ordering::SeqCst);
            Ok(self.return_summary.clone())
        }
    }

    fn make_summary() -> CostSummary {
        CostSummary {
            last_30_days_total_usd: 1.23,
            last_30_days_runs: 5,
            last_year_total_usd: 10.0,
            last_year_runs: 42,
            computed_at: Utc::now(),
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
}
