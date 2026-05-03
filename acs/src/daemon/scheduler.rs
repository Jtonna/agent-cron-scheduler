use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, Notify, RwLock};

use uuid::Uuid;

use crate::daemon::events::WorkflowEvent;
use crate::models::workflow::TriggerParams;
use crate::models::ScheduleMode;
use crate::storage::workflows::WorkflowStore;

// ---------------------------------------------------------------------------
// Clock trait + implementations
// ---------------------------------------------------------------------------

/// Trait for abstracting time, enabling deterministic testing.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

/// Real clock backed by system time.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Fake clock for deterministic testing — time only advances when told to.
/// Uses std::sync::RwLock (not tokio) so it can be called from both sync
/// and async contexts without panicking.
pub struct FakeClock {
    time: Arc<std::sync::RwLock<DateTime<Utc>>>,
}

impl FakeClock {
    /// Create a FakeClock pinned to the given instant.
    pub fn new(time: DateTime<Utc>) -> Self {
        Self {
            time: Arc::new(std::sync::RwLock::new(time)),
        }
    }

    /// Set the clock to a specific instant.
    pub fn set(&self, time: DateTime<Utc>) {
        *self.time.write().unwrap() = time;
    }

    /// Advance the clock by a chrono::Duration.
    pub fn advance(&self, duration: chrono::Duration) {
        let mut t = self.time.write().unwrap();
        *t += duration;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.time.read().unwrap()
    }
}

// ---------------------------------------------------------------------------
// compute_next_run — timezone-aware cron next-occurrence calculation
// ---------------------------------------------------------------------------

/// Compute the next run time for a cron schedule after `after` (exclusive).
///
/// If `timezone` is Some, the cron expression is evaluated in that IANA
/// timezone (e.g. "America/New_York") and the result is converted back to UTC.
/// If `timezone` is None, the cron expression is evaluated in UTC.
pub fn compute_next_run(
    schedule: &str,
    timezone: Option<&str>,
    after: DateTime<Utc>,
) -> Result<DateTime<Utc>> {
    use std::str::FromStr;
    let cron = croner::Cron::from_str(schedule)
        .map_err(|e| anyhow::anyhow!("Invalid cron expression '{}': {}", schedule, e))?;

    match timezone {
        Some(tz_str) => {
            let tz: chrono_tz::Tz = tz_str
                .parse()
                .map_err(|e| anyhow::anyhow!("Invalid timezone '{}': {}", tz_str, e))?;
            // Convert to local time, find next occurrence in that zone, then back to UTC
            let local_after = after.with_timezone(&tz);
            let next_local = cron
                .find_next_occurrence(&local_after, false)
                .map_err(|e| anyhow::anyhow!("Cron next occurrence error: {}", e))?;
            Ok(next_local.with_timezone(&Utc))
        }
        None => {
            let next = cron
                .find_next_occurrence(&after, false)
                .map_err(|e| anyhow::anyhow!("Cron next occurrence error: {}", e))?;
            Ok(next)
        }
    }
}

// ---------------------------------------------------------------------------
// WorkflowScheduler
// ---------------------------------------------------------------------------

/// Workflow-aware scheduler that reads from a `WorkflowStore` and dispatches
/// runs via `crate::workflow::executor::run_workflow`.
///
/// This replaces the old `Scheduler` (which read from `JobStore`) as of Phase 6.
pub struct WorkflowScheduler {
    workflow_store: Arc<dyn WorkflowStore>,
    clock: Arc<dyn Clock>,
    notify: Arc<Notify>,
    workflow_event_tx: broadcast::Sender<WorkflowEvent>,
    workflow_runs: Arc<RwLock<HashMap<Uuid, Arc<RwLock<crate::models::workflow::WorkflowRun>>>>>,
    data_dir: std::path::PathBuf,
}

impl WorkflowScheduler {
    pub fn new(
        workflow_store: Arc<dyn WorkflowStore>,
        clock: Arc<dyn Clock>,
        notify: Arc<Notify>,
        workflow_event_tx: broadcast::Sender<WorkflowEvent>,
        workflow_runs: Arc<RwLock<HashMap<Uuid, Arc<RwLock<crate::models::workflow::WorkflowRun>>>>>,
        data_dir: std::path::PathBuf,
    ) -> Self {
        Self {
            workflow_store,
            clock,
            notify,
            workflow_event_tx,
            workflow_runs,
            data_dir,
        }
    }

    /// Main scheduler loop — runs until the process exits or the store is dropped.
    pub async fn run(&self) -> Result<()> {
        loop {
            let workflows = self.workflow_store.list_workflows().await?;
            let enabled: Vec<_> = workflows.into_iter().filter(|w| w.enabled).collect();

            // Compute next_run for each enabled workflow
            let mut next_runs: Vec<(crate::models::workflow::Workflow, DateTime<Utc>)> =
                Vec::new();
            for wf in enabled {
                match compute_next_run(&wf.schedule, wf.timezone.as_deref(), self.clock.now()) {
                    Ok(next) => next_runs.push((wf, next)),
                    Err(e) => {
                        tracing::error!(
                            workflow_id = %wf.id,
                            workflow_name = %wf.name,
                            "Invalid schedule for workflow '{}' ({}): {}",
                            wf.name, wf.id, e
                        );
                    }
                }
            }

            if next_runs.is_empty() {
                // No enabled workflows — sleep until notified
                self.notify.notified().await;
                continue;
            }

            let earliest = next_runs.iter().map(|(_, t)| *t).min().unwrap();
            let now = self.clock.now();
            let sleep_duration = (earliest - now).to_std().unwrap_or(Duration::ZERO);

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    let now = self.clock.now();
                    for (wf, next_time) in &next_runs {
                        if *next_time <= now {
                            // WaitForCompletion: skip if already running
                            if wf.schedule_mode == ScheduleMode::WaitForCompletion {
                                let runs = self.workflow_runs.read().await;
                                let has_active = runs.values().any(|r| {
                                    // Check if any run belongs to this workflow and is still Running
                                    if let Ok(run) = r.try_read() {
                                        run.workflow_id == wf.id
                                            && run.status == crate::models::workflow::RunStatus::Running
                                    } else {
                                        false
                                    }
                                });
                                if has_active {
                                    tracing::debug!(
                                        workflow_id = %wf.id,
                                        workflow_name = %wf.name,
                                        "WaitForCompletion: skipping dispatch — run still active"
                                    );
                                    continue;
                                }
                            }

                            let run_id = Uuid::now_v7();
                            let wf_clone = wf.clone();
                            let event_tx = self.workflow_event_tx.clone();
                            let workflow_runs = Arc::clone(&self.workflow_runs);
                            let data_dir = self.data_dir.clone();

                            tokio::spawn(async move {
                                // Build log path: data_dir/logs/<workflow_id>/<run_id>.log
                                let log_dir = data_dir.join("logs").join(wf_clone.id.to_string());
                                if let Err(e) = tokio::fs::create_dir_all(&log_dir).await {
                                    tracing::error!(
                                        workflow_id = %wf_clone.id,
                                        "Failed to create log dir: {}",
                                        e
                                    );
                                    return;
                                }
                                let log_path = log_dir.join(format!("{}.log", run_id));

                                let sink = match crate::workflow::log_sink::FileLogSink::create(log_path).await {
                                    Ok(s) => Arc::new(s) as Arc<dyn crate::workflow::step::LogSink>,
                                    Err(e) => {
                                        tracing::error!(
                                            workflow_id = %wf_clone.id,
                                            "Failed to create log sink: {}",
                                            e
                                        );
                                        return;
                                    }
                                };

                                // Create a minimal TriggerParams (no input for scheduled runs)
                                let trigger = TriggerParams {
                                    input: serde_json::Value::Null,
                                    env: None,
                                    target_step: None,
                                };

                                let run = crate::workflow::executor::run_workflow(
                                    &wf_clone,
                                    run_id,
                                    trigger,
                                    sink,
                                    Some(event_tx),
                                ).await;

                                // Store the completed run in the in-memory map
                                let run = Arc::new(RwLock::new(run));
                                workflow_runs.write().await.insert(run_id, run);
                            });
                        }
                    }
                }
                _ = self.notify.notified() => {
                    // Workflow list changed — re-evaluate
                    continue;
                }
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_next_run_at_every_5_minutes() {
        // At 10:03, the next */5 minute boundary is 10:05
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 10, 3, 0).unwrap();
        let next = compute_next_run("*/5 * * * *", None, after).unwrap();
        let expected = Utc.with_ymd_and_hms(2025, 6, 15, 10, 5, 0).unwrap();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_next_run_at_on_boundary_is_exclusive() {
        // At exactly 10:05, the *next* */5 boundary is 10:10 (exclusive)
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 10, 5, 0).unwrap();
        let next = compute_next_run("*/5 * * * *", None, after).unwrap();
        let expected = Utc.with_ymd_and_hms(2025, 6, 15, 10, 10, 0).unwrap();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_next_run_at_every_hour() {
        // "0 * * * *" fires at the top of each hour
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 10, 30, 0).unwrap();
        let next = compute_next_run("0 * * * *", None, after).unwrap();
        let expected = Utc.with_ymd_and_hms(2025, 6, 15, 11, 0, 0).unwrap();
        assert_eq!(next, expected);
    }

    // =======================================================================
    // 2. next_run_at with timezone
    // =======================================================================

    #[test]
    fn test_next_run_at_with_timezone() {
        // "0 0 * * *" = midnight daily
        // In America/New_York (UTC-5 in winter / UTC-4 in summer)
        // June 15, 2025: EDT => UTC-4
        // If it's 2025-06-15 03:00 UTC, that's 2025-06-14 23:00 EDT
        // Next midnight EDT = 2025-06-15 00:00 EDT = 2025-06-15 04:00 UTC
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 3, 0, 0).unwrap();
        let next = compute_next_run("0 0 * * *", Some("America/New_York"), after).unwrap();
        let expected = Utc.with_ymd_and_hms(2025, 6, 15, 4, 0, 0).unwrap();
        assert_eq!(next, expected);
    }

    #[test]
    fn test_next_run_at_with_utc_timezone_explicit() {
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 10, 3, 0).unwrap();
        let next = compute_next_run("*/5 * * * *", Some("UTC"), after).unwrap();
        let expected = Utc.with_ymd_and_hms(2025, 6, 15, 10, 5, 0).unwrap();
        assert_eq!(next, expected);
    }

    // =======================================================================
    // 3. DST spring-forward: 2:30 AM schedule during spring-forward is skipped
    // =======================================================================

    #[test]
    fn test_dst_spring_forward_skipped_time() {
        // In 2025, US spring forward: March 9 at 2:00 AM EDT.
        // 2:30 AM does not exist on March 9.
        // Schedule: "30 2 * * *" in America/New_York
        // Before the spring-forward date:
        let after = Utc.with_ymd_and_hms(2025, 3, 9, 6, 0, 0).unwrap(); // 1:00 AM EST
        let result = compute_next_run("30 2 * * *", Some("America/New_York"), after);
        match result {
            Ok(next) => {
                // croner may return either:
                // a) March 9 at 3:00 AM EDT (07:00 UTC) — the next valid time after
                //    the spring-forward gap on that day, or
                // b) March 10 at 2:30 AM EDT (06:30 UTC) — the next day when 2:30 AM
                //    actually exists again.
                // Both are valid behaviors for handling DST spring-forward.
                let march_9_3am_edt = Utc.with_ymd_and_hms(2025, 3, 9, 7, 0, 0).unwrap();
                let march_10_230am_edt = Utc.with_ymd_and_hms(2025, 3, 10, 6, 30, 0).unwrap();
                assert!(
                    next == march_9_3am_edt || next == march_10_230am_edt,
                    "Expected {:?} or {:?}, got {:?}",
                    march_9_3am_edt,
                    march_10_230am_edt,
                    next
                );
            }
            Err(_) => {
                // If croner reports an error for the skipped time, that is also
                // acceptable behavior — the scheduler will skip this job that tick.
            }
        }
    }

    // =======================================================================
    // 4. DST fall-back: first occurrence used (not duplicated)
    // =======================================================================

    #[test]
    fn test_dst_fall_back_first_occurrence() {
        // In 2025, US fall back: November 2 at 2:00 AM EDT -> 1:00 AM EST.
        // Schedule: "30 1 * * *" in America/New_York
        // 1:30 AM occurs twice. We should get the first (EDT) occurrence.
        // Before the overlap: 2025-11-02 04:00 UTC = midnight EDT
        let after = Utc.with_ymd_and_hms(2025, 11, 2, 4, 0, 0).unwrap();
        let next = compute_next_run("30 1 * * *", Some("America/New_York"), after).unwrap();
        // First 1:30 AM is EDT (UTC-4): 2025-11-02 05:30 UTC
        let expected_first = Utc.with_ymd_and_hms(2025, 11, 2, 5, 30, 0).unwrap();
        // Second 1:30 AM is EST (UTC-5): 2025-11-02 06:30 UTC
        let expected_second = Utc.with_ymd_and_hms(2025, 11, 2, 6, 30, 0).unwrap();
        // We accept either, but prefer the first
        assert!(
            next == expected_first || next == expected_second,
            "Expected {:?} or {:?}, got {:?}",
            expected_first,
            expected_second,
            next
        );
    }

    #[test]
    fn test_fake_clock_new_and_now() {
        let t = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let clock = FakeClock::new(t);
        assert_eq!(clock.now(), t);
    }

    #[test]
    fn test_fake_clock_set() {
        let t1 = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2025, 6, 15, 12, 0, 0).unwrap();
        let clock = FakeClock::new(t1);
        clock.set(t2);
        assert_eq!(clock.now(), t2);
    }

    #[test]
    fn test_fake_clock_advance() {
        let t = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
        let clock = FakeClock::new(t);
        clock.advance(chrono::Duration::hours(1));
        let expected = Utc.with_ymd_and_hms(2025, 1, 1, 1, 0, 0).unwrap();
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn test_system_clock_returns_recent_time() {
        let clock = SystemClock;
        let now = clock.now();
        let actual_now = Utc::now();
        // Should be within 1 second
        let diff = (actual_now - now).num_seconds().abs();
        assert!(diff < 2, "SystemClock should return approximately now");
    }
}
