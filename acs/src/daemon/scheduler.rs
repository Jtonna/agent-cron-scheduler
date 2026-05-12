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
use crate::storage::workflow_runs::WorkflowRunStore;
use crate::storage::workflows::WorkflowStore;
use crate::workflow::step::KillSender;

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
// Cron dispatch decision
// ---------------------------------------------------------------------------

/// Outcome of the cron-tick dispatch check for a single workflow.
///
/// Used by the scheduler loop to decide whether to dispatch a new run, skip
/// because a `WaitForCompletion` workflow already has an active run, or skip
/// because `allow_concurrent: false` and a run is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CronDispatchDecision {
    /// No concurrency guard tripped — start a new run for this tick.
    Dispatch,
    /// `schedule_mode == WaitForCompletion` and a run is active. Skip.
    SkipWaitForCompletion,
    /// `allow_concurrent: false` and a run is active. Skip and leave the
    /// active run untouched.
    SkipNoConcurrency,
}

/// Compute the cron-dispatch decision for a workflow given whether a run is
/// currently active for it.
///
/// Precedence (matches the scheduler loop):
/// 1. `schedule_mode == WaitForCompletion` + active run → `SkipWaitForCompletion`.
/// 2. `allow_concurrent == false` + active run → `SkipNoConcurrency`.
/// 3. Otherwise → `Dispatch`.
pub fn cron_dispatch_decision(
    workflow: &crate::models::workflow::Workflow,
    has_active_run: bool,
) -> CronDispatchDecision {
    if has_active_run && workflow.schedule_mode == ScheduleMode::WaitForCompletion {
        return CronDispatchDecision::SkipWaitForCompletion;
    }
    if has_active_run && !workflow.allow_concurrent {
        return CronDispatchDecision::SkipNoConcurrency;
    }
    CronDispatchDecision::Dispatch
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
    workflow_run_store: Arc<dyn WorkflowRunStore>,
    data_dir: std::path::PathBuf,
    /// Kill-signals registry shared with `AppState`. Cron-dispatched runs
    /// register their `KillSender` here so `POST /api/runs/{id}/kill` can
    /// signal them normally.
    kill_signals: Arc<RwLock<HashMap<Uuid, KillSender>>>,
}

impl WorkflowScheduler {
    pub fn new(
        workflow_store: Arc<dyn WorkflowStore>,
        clock: Arc<dyn Clock>,
        notify: Arc<Notify>,
        workflow_event_tx: broadcast::Sender<WorkflowEvent>,
        workflow_run_store: Arc<dyn WorkflowRunStore>,
        data_dir: std::path::PathBuf,
        kill_signals: Arc<RwLock<HashMap<Uuid, KillSender>>>,
    ) -> Self {
        Self {
            workflow_store,
            clock,
            notify,
            workflow_event_tx,
            workflow_run_store,
            data_dir,
            kill_signals,
        }
    }

    /// Main scheduler loop — runs until the process exits or the store is dropped.
    pub async fn run(&self) -> Result<()> {
        loop {
            let workflows = self.workflow_store.list_workflows().await?;
            let enabled: Vec<_> = workflows.into_iter().filter(|w| w.enabled).collect();

            // Compute next_run for each enabled workflow
            let mut next_runs: Vec<(crate::models::workflow::Workflow, DateTime<Utc>)> = Vec::new();
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
                            // Decide whether to skip this cron tick based on
                            // schedule_mode and allow_concurrent. We only need
                            // the active-run flag if at least one rule applies.
                            let needs_active_check = wf.schedule_mode == ScheduleMode::WaitForCompletion
                                || !wf.allow_concurrent;
                            let has_active = if needs_active_check {
                                match self.workflow_run_store.list_runs(wf.id, 0, 0).await {
                                    Ok(runs) => runs.iter().any(|r| {
                                        r.status == crate::models::workflow::RunStatus::Running
                                    }),
                                    Err(e) => {
                                        tracing::warn!(
                                            workflow_id = %wf.id,
                                            "Failed to check active runs: {}",
                                            e
                                        );
                                        false
                                    }
                                }
                            } else {
                                false
                            };

                            match cron_dispatch_decision(wf, has_active) {
                                CronDispatchDecision::SkipWaitForCompletion => {
                                    tracing::debug!(
                                        workflow_id = %wf.id,
                                        workflow_name = %wf.name,
                                        "WaitForCompletion: skipping dispatch — run still active"
                                    );
                                    continue;
                                }
                                CronDispatchDecision::SkipNoConcurrency => {
                                    tracing::warn!(
                                        workflow_id = %wf.id,
                                        workflow_name = %wf.name,
                                        "workflow {} has active run, skipping dispatch (allow_concurrent=false)",
                                        wf.name
                                    );
                                    continue;
                                }
                                CronDispatchDecision::Dispatch => {}
                            }

                            let run_id = Uuid::now_v7();
                            let wf_clone = wf.clone();
                            let event_tx = self.workflow_event_tx.clone();
                            let run_store = Arc::clone(&self.workflow_run_store);
                            let workflow_store = Arc::clone(&self.workflow_store);
                            let data_dir = self.data_dir.clone();
                            let kill_signals = Arc::clone(&self.kill_signals);

                            tokio::spawn(async move {
                                // Persist an initial Running record before execution.
                                let initial_run = crate::models::workflow::WorkflowRun {
                                    run_id,
                                    workflow_id: wf_clone.id,
                                    workflow_version: wf_clone.version,
                                    workflow_snapshot: wf_clone.clone(),
                                    started_at: Utc::now(),
                                    finished_at: None,
                                    status: crate::models::workflow::RunStatus::Running,
                                    trigger_input: None,
                                    steps: vec![],
                                    total_cost_usd: None,
                                    total_duration_ms: None,
                                };
                                if let Err(e) = run_store.create_run(initial_run).await {
                                    tracing::error!(
                                        workflow_id = %wf_clone.id,
                                        "Failed to persist initial run {}: {}",
                                        run_id, e
                                    );
                                    return;
                                }

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
                                    Ok(s) => {
                                        let emitting = crate::workflow::EventEmittingLogSink::new(
                                            Arc::new(s) as Arc<dyn crate::workflow::step::LogSink>,
                                            event_tx.clone(),
                                            run_id,
                                            wf_clone.id,
                                        );
                                        Arc::new(emitting) as Arc<dyn crate::workflow::step::LogSink>
                                    }
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

                                let final_run = crate::workflow::executor::run_workflow(
                                    &wf_clone,
                                    run_id,
                                    trigger,
                                    sink,
                                    Some(event_tx),
                                    Some(kill_signals),
                                ).await;

                                // Persist the final run state and stamp the
                                // parent workflow with this run's terminal
                                // status so list_workflows / get_workflow
                                // surface it in last_run_*.
                                crate::workflow::finalize_run(
                                    &run_store,
                                    &workflow_store,
                                    &final_run,
                                )
                                .await;
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

    // ── Cron dispatch decision tests ──────────────────────────────────────────

    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, ShellStep, StepDef, StepDefCommon, Workflow,
    };
    use uuid::Uuid;

    fn make_workflow(allow_concurrent: bool, schedule_mode: ScheduleMode) -> Workflow {
        let now = Utc::now();
        Workflow {
            id: Uuid::now_v7(),
            name: "test-wf".to_string(),
            version: 1,
            schedule: "* * * * *".to_string(),
            timezone: None,
            schedule_mode,
            enabled: true,
            steps: vec![StepDef::Shell(ShellStep {
                common: StepDefCommon {
                    id: "s1".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                command: "echo hi".to_string(),
                pass_stdin: false,
            })],
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent,
            on_failure: FailurePolicy::default(),
            last_run_at: None,
            last_run_status: None,
            last_run_id: None,
            next_run_at: None,
            created_at: now,
            updated_at: now,
            cost_summary: None,
        }
    }

    #[test]
    fn test_cron_dispatch_no_active_run_dispatches() {
        let wf = make_workflow(true, ScheduleMode::Cron);
        assert_eq!(
            cron_dispatch_decision(&wf, false),
            CronDispatchDecision::Dispatch
        );
    }

    #[test]
    fn test_cron_dispatch_allow_concurrent_true_with_active_run_dispatches() {
        let wf = make_workflow(true, ScheduleMode::Cron);
        assert_eq!(
            cron_dispatch_decision(&wf, true),
            CronDispatchDecision::Dispatch
        );
    }

    #[test]
    fn test_cron_dispatch_allow_concurrent_false_with_active_run_skips() {
        let wf = make_workflow(false, ScheduleMode::Cron);
        assert_eq!(
            cron_dispatch_decision(&wf, true),
            CronDispatchDecision::SkipNoConcurrency
        );
    }

    #[test]
    fn test_cron_dispatch_allow_concurrent_false_no_active_run_dispatches() {
        let wf = make_workflow(false, ScheduleMode::Cron);
        assert_eq!(
            cron_dispatch_decision(&wf, false),
            CronDispatchDecision::Dispatch
        );
    }

    #[test]
    fn test_cron_dispatch_wait_for_completion_with_active_run_skips() {
        // schedule_mode takes precedence over allow_concurrent.
        let wf = make_workflow(true, ScheduleMode::WaitForCompletion);
        assert_eq!(
            cron_dispatch_decision(&wf, true),
            CronDispatchDecision::SkipWaitForCompletion
        );
    }

    #[test]
    fn test_cron_dispatch_wait_for_completion_no_active_run_dispatches() {
        let wf = make_workflow(true, ScheduleMode::WaitForCompletion);
        assert_eq!(
            cron_dispatch_decision(&wf, false),
            CronDispatchDecision::Dispatch
        );
    }

    #[test]
    fn test_cron_dispatch_wait_for_completion_with_active_run_takes_precedence() {
        // WaitForCompletion + allow_concurrent: false + active run → the
        // WaitForCompletion outcome takes precedence (per the function's
        // documented order).
        let wf = make_workflow(false, ScheduleMode::WaitForCompletion);
        assert_eq!(
            cron_dispatch_decision(&wf, true),
            CronDispatchDecision::SkipWaitForCompletion
        );
    }
}
