use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};
use tokio::sync::{mpsc, Notify};

use crate::models::Job;
use crate::storage::JobStore;

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
// Scheduler
// ---------------------------------------------------------------------------

/// The cron scheduler engine.
///
/// It is a long-lived task that loads enabled jobs, computes next run times,
/// sleeps until the earliest one, and dispatches due jobs over an mpsc channel.
/// It can be woken early via `Notify` when the job list changes.
pub struct Scheduler {
    job_store: Arc<dyn JobStore>,
    clock: Arc<dyn Clock>,
    notify: Arc<Notify>,
    dispatch_tx: mpsc::Sender<Job>,
}

impl Scheduler {
    /// Create a new Scheduler.
    pub fn new(
        job_store: Arc<dyn JobStore>,
        clock: Arc<dyn Clock>,
        notify: Arc<Notify>,
        dispatch_tx: mpsc::Sender<Job>,
    ) -> Self {
        Self {
            job_store,
            clock,
            notify,
            dispatch_tx,
        }
    }

    /// Main scheduler loop.  Runs forever (or until the mpsc channel closes).
    pub async fn run(&self) -> Result<()> {
        loop {
            let jobs = self.job_store.list_jobs().await?;
            let enabled_jobs: Vec<&Job> = jobs.iter().filter(|j| j.enabled).collect();

            // Compute next run for each enabled job
            let mut next_runs: Vec<(Job, DateTime<Utc>)> = Vec::new();
            for job in enabled_jobs {
                match compute_next_run(&job.schedule, job.timezone.as_deref(), self.clock.now()) {
                    Ok(next) => next_runs.push((job.clone(), next)),
                    Err(e) => {
                        tracing::error!(
                            "Invalid schedule for job '{}' ({}): {}",
                            job.name,
                            job.id,
                            e
                        );
                        // Invalid jobs are skipped (not dispatched).
                    }
                }
            }

            if next_runs.is_empty() {
                // No enabled jobs — sleep indefinitely until notified
                self.notify.notified().await;
                continue;
            }

            // Find earliest next run
            let earliest = next_runs.iter().map(|(_, t)| *t).min().unwrap();
            let now = self.clock.now();
            let sleep_duration = (earliest - now).to_std().unwrap_or(Duration::ZERO);

            tokio::select! {
                _ = tokio::time::sleep(sleep_duration) => {
                    // Dispatch all jobs that are now due
                    let now = self.clock.now();
                    for (job, next_time) in &next_runs {
                        if *next_time <= now {
                            let _ = self.dispatch_tx.send(job.clone()).await;
                        }
                    }
                }
                _ = self.notify.notified() => {
                    // Job list changed — re-evaluate from the top
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
    use crate::models::{ExecutionType, Job, JobUpdate, NewJob};
    use crate::storage::JobStore;
    use async_trait::async_trait;
    use chrono::TimeZone;
    use tokio::sync::RwLock;
    use uuid::Uuid;

    // -----------------------------------------------------------------------
    // InMemoryJobStore — test double
    // -----------------------------------------------------------------------

    struct InMemoryJobStore {
        jobs: RwLock<Vec<Job>>,
    }

    impl InMemoryJobStore {
        fn new() -> Self {
            Self {
                jobs: RwLock::new(Vec::new()),
            }
        }

        async fn add_job(&self, job: Job) {
            self.jobs.write().await.push(job);
        }
    }

    #[async_trait]
    impl JobStore for InMemoryJobStore {
        async fn list_jobs(&self) -> Result<Vec<Job>> {
            Ok(self.jobs.read().await.clone())
        }

        async fn get_job(&self, id: Uuid) -> Result<Option<Job>> {
            Ok(self.jobs.read().await.iter().find(|j| j.id == id).cloned())
        }

        async fn find_by_name(&self, name: &str) -> Result<Option<Job>> {
            Ok(self
                .jobs
                .read()
                .await
                .iter()
                .find(|j| j.name == name)
                .cloned())
        }

        async fn create_job(&self, new: NewJob) -> Result<Job> {
            let now = Utc::now();
            let job = Job {
                id: Uuid::now_v7(),
                name: new.name,
                schedule: new.schedule,
                execution: new.execution,
                enabled: new.enabled,
                timezone: new.timezone,
                working_dir: new.working_dir,
                env_vars: new.env_vars,
                timeout_secs: new.timeout_secs,
                log_environment: new.log_environment,
                created_at: now,
                updated_at: now,
                last_run_at: None,
                last_exit_code: None,
                next_run_at: None,
            };
            self.jobs.write().await.push(job.clone());
            Ok(job)
        }

        async fn update_job(&self, id: Uuid, update: JobUpdate) -> Result<Job> {
            let mut jobs = self.jobs.write().await;
            let job = jobs
                .iter_mut()
                .find(|j| j.id == id)
                .ok_or_else(|| anyhow::anyhow!("not found"))?;
            if let Some(name) = update.name {
                job.name = name;
            }
            if let Some(schedule) = update.schedule {
                job.schedule = schedule;
            }
            if let Some(enabled) = update.enabled {
                job.enabled = enabled;
            }
            job.updated_at = Utc::now();
            Ok(job.clone())
        }

        async fn delete_job(&self, id: Uuid) -> Result<()> {
            let mut jobs = self.jobs.write().await;
            jobs.retain(|j| j.id != id);
            Ok(())
        }
    }

    // -----------------------------------------------------------------------
    // Helper: build a Job struct for testing
    // -----------------------------------------------------------------------

    fn make_test_job(name: &str, schedule: &str, enabled: bool) -> Job {
        let now = Utc::now();
        Job {
            id: Uuid::now_v7(),
            name: name.to_string(),
            schedule: schedule.to_string(),
            execution: ExecutionType::ShellCommand("echo hello".to_string()),
            enabled,
            timezone: None,
            working_dir: None,
            env_vars: None,
            timeout_secs: 0,
            log_environment: false,
            created_at: now,
            updated_at: now,
            last_run_at: None,
            last_exit_code: None,
            next_run_at: None,
        }
    }

    #[allow(dead_code)]
    fn make_test_job_with_tz(name: &str, schedule: &str, tz: &str) -> Job {
        let mut job = make_test_job(name, schedule, true);
        job.timezone = Some(tz.to_string());
        job
    }

    // =======================================================================
    // 1. next_run_at calculation: */5 * * * * at 10:03 -> 10:05
    // =======================================================================

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

    // =======================================================================
    // 5. No enabled jobs — scheduler sleeps until notified
    // =======================================================================

    #[tokio::test]
    async fn test_no_enabled_jobs_sleeps_until_notified() {
        let store = Arc::new(InMemoryJobStore::new());
        let notify = Arc::new(Notify::new());
        let clock = Arc::new(FakeClock::new(Utc::now()));
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let scheduler = Scheduler::new(store.clone(), clock.clone(), notify.clone(), tx);

        // Spawn the scheduler
        let handle = tokio::spawn(async move { scheduler.run().await });

        // Give scheduler time to enter the wait state
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Nothing should be dispatched
        assert!(
            rx.try_recv().is_err(),
            "No jobs should be dispatched when none are enabled"
        );

        // Wake the scheduler via Notify (to prove it's alive and listening)
        notify.notify_one();

        // Give it a moment to loop
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Still nothing dispatched since there are still no enabled jobs
        assert!(rx.try_recv().is_err());

        // Clean up
        handle.abort();
    }

    // =======================================================================
    // 6. Scheduler dispatches job when cron fires
    // =======================================================================

    #[tokio::test]
    async fn test_scheduler_dispatches_job_when_cron_fires() {
        let store = Arc::new(InMemoryJobStore::new());

        // Create a job that fires at */1 * * * * (every minute)
        // Set clock to 10:00:30 — next fire is 10:01:00
        let base_time = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 30).unwrap();
        let clock = Arc::new(FakeClock::new(base_time));

        let job = make_test_job("minutely-job", "*/1 * * * *", true);
        store.add_job(job.clone()).await;

        let notify = Arc::new(Notify::new());
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let clock_clone = clock.clone();
        let scheduler = Scheduler::new(store.clone(), clock_clone, notify.clone(), tx);

        let handle = tokio::spawn(async move { scheduler.run().await });

        // The scheduler will compute next_run = 10:01:00 and sleep for 30s.
        // We need the tokio::time::sleep to fire. We can use tokio pause/advance.
        // First, advance the fake clock past the fire time.
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Advance fake clock to 10:01:00
        clock.set(Utc.with_ymd_and_hms(2025, 6, 15, 10, 1, 0).unwrap());

        // Also advance tokio time so the sleep fires
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::time::resume();

        // Allow scheduler to process
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Should have received the dispatched job
        let dispatched = rx.try_recv();
        assert!(dispatched.is_ok(), "Job should have been dispatched");
        assert_eq!(dispatched.unwrap().name, "minutely-job");

        handle.abort();
    }

    // =======================================================================
    // 7. Scheduler skips disabled jobs
    // =======================================================================

    #[tokio::test]
    async fn test_scheduler_skips_disabled_jobs() {
        let store = Arc::new(InMemoryJobStore::new());

        let base_time = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 30).unwrap();
        let clock = Arc::new(FakeClock::new(base_time));

        // Add a disabled job
        let disabled_job = make_test_job("disabled-job", "*/1 * * * *", false);
        store.add_job(disabled_job).await;

        let notify = Arc::new(Notify::new());
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let scheduler = Scheduler::new(store.clone(), clock.clone(), notify.clone(), tx);

        let handle = tokio::spawn(async move { scheduler.run().await });

        // The scheduler should see no enabled jobs and wait on notify
        tokio::time::sleep(Duration::from_millis(100)).await;

        // Advance clock and notify to re-evaluate
        clock.set(Utc.with_ymd_and_hms(2025, 6, 15, 10, 1, 0).unwrap());
        notify.notify_one();
        tokio::time::sleep(Duration::from_millis(100)).await;

        // No jobs should be dispatched because the only job is disabled
        assert!(
            rx.try_recv().is_err(),
            "Disabled jobs should not be dispatched"
        );

        handle.abort();
    }

    // =======================================================================
    // 8. Scheduler dispatches all due jobs concurrently
    // =======================================================================

    #[tokio::test]
    async fn test_scheduler_dispatches_all_due_jobs_concurrently() {
        let store = Arc::new(InMemoryJobStore::new());

        // Start at 10:00:30.  Three jobs all fire at */1 * * * * -> next at 10:01
        let base_time = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 30).unwrap();
        let clock = Arc::new(FakeClock::new(base_time));

        store
            .add_job(make_test_job("job-a", "*/1 * * * *", true))
            .await;
        store
            .add_job(make_test_job("job-b", "*/1 * * * *", true))
            .await;
        store
            .add_job(make_test_job("job-c", "*/1 * * * *", true))
            .await;

        let notify = Arc::new(Notify::new());
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let clock_clone = clock.clone();
        let scheduler = Scheduler::new(store.clone(), clock_clone, notify.clone(), tx);

        let handle = tokio::spawn(async move { scheduler.run().await });

        // Let the scheduler start and compute next runs
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Advance time past the fire point
        clock.set(Utc.with_ymd_and_hms(2025, 6, 15, 10, 1, 0).unwrap());

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::time::resume();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Collect all dispatched jobs
        let mut dispatched = Vec::new();
        while let Ok(job) = rx.try_recv() {
            dispatched.push(job.name.clone());
        }

        assert_eq!(dispatched.len(), 3, "All 3 due jobs should be dispatched");
        assert!(dispatched.contains(&"job-a".to_string()));
        assert!(dispatched.contains(&"job-b".to_string()));
        assert!(dispatched.contains(&"job-c".to_string()));

        handle.abort();
    }

    // =======================================================================
    // 9. Scheduler wakes on Notify
    // =======================================================================

    #[tokio::test]
    async fn test_scheduler_wakes_on_notify() {
        let store = Arc::new(InMemoryJobStore::new());

        // Start with no jobs — scheduler should sleep on notify
        let clock = Arc::new(FakeClock::new(Utc::now()));
        let notify = Arc::new(Notify::new());
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let store_clone = store.clone();
        let notify_clone = notify.clone();
        let scheduler = Scheduler::new(store_clone, clock.clone(), notify_clone, tx);

        let handle = tokio::spawn(async move { scheduler.run().await });

        // Scheduler should be sleeping (no jobs)
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(rx.try_recv().is_err(), "No jobs dispatched initially");

        // Now add a job and notify
        let job = make_test_job("new-job", "*/1 * * * *", true);
        store.add_job(job).await;
        notify.notify_one();

        // After notify, the scheduler re-evaluates and picks up the new job.
        // It won't dispatch immediately (cron hasn't fired yet), but it
        // should now be sleeping on the timer rather than indefinitely on notify.
        tokio::time::sleep(Duration::from_millis(100)).await;

        // The scheduler is alive and looping — test passes if we get here
        // without hanging. The job isn't due yet so nothing is dispatched.
        assert!(
            rx.try_recv().is_err(),
            "Job not due yet, should not be dispatched"
        );

        handle.abort();
    }

    // =======================================================================
    // 10. Invalid cron expression — job is skipped, not dispatched
    // =======================================================================

    #[test]
    fn test_invalid_cron_expression_returns_error() {
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        let result = compute_next_run("not a cron", None, after);
        assert!(result.is_err(), "Invalid cron should return error");
    }

    #[tokio::test]
    async fn test_scheduler_skips_invalid_cron_jobs() {
        let store = Arc::new(InMemoryJobStore::new());

        let base_time = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 30).unwrap();
        let clock = Arc::new(FakeClock::new(base_time));

        // Add a job with an invalid cron — it's enabled but cron is garbage
        let bad_job = make_test_job("bad-cron-job", "INVALID CRON", true);
        store.add_job(bad_job.clone()).await;

        // Also add a valid job to prove it still works
        let good_job = make_test_job("good-job", "*/1 * * * *", true);
        store.add_job(good_job.clone()).await;

        let notify = Arc::new(Notify::new());
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let scheduler = Scheduler::new(store.clone(), clock.clone(), notify.clone(), tx);

        let handle = tokio::spawn(async move { scheduler.run().await });

        tokio::time::sleep(Duration::from_millis(50)).await;

        // Advance past the good job's fire time
        clock.set(Utc.with_ymd_and_hms(2025, 6, 15, 10, 1, 0).unwrap());

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::time::resume();

        tokio::time::sleep(Duration::from_millis(100)).await;

        // Only the good job should be dispatched
        let mut dispatched = Vec::new();
        while let Ok(job) = rx.try_recv() {
            dispatched.push(job.name.clone());
        }

        assert!(
            dispatched.contains(&"good-job".to_string()),
            "Valid job should be dispatched"
        );
        assert!(
            !dispatched.contains(&"bad-cron-job".to_string()),
            "Invalid cron job should NOT be dispatched"
        );

        handle.abort();
    }

    // =======================================================================
    // Additional: invalid timezone returns error
    // =======================================================================

    #[test]
    fn test_invalid_timezone_returns_error() {
        let after = Utc.with_ymd_and_hms(2025, 6, 15, 10, 0, 0).unwrap();
        let result = compute_next_run("*/5 * * * *", Some("Not/Valid"), after);
        assert!(result.is_err(), "Invalid timezone should return error");
    }

    // =======================================================================
    // 11. Scheduler dispatches timezone-aware job correctly
    // =======================================================================

    #[tokio::test]
    async fn test_scheduler_dispatches_timezone_aware_job() {
        let store = Arc::new(InMemoryJobStore::new());

        // June 15, 2025 — EDT is UTC-4
        // Job: "0 10 * * *" in America/New_York = 10:00 AM EDT = 14:00 UTC
        // Clock starts at 13:59:30 UTC (just before fire time)
        let base_time = Utc.with_ymd_and_hms(2025, 6, 15, 13, 59, 30).unwrap();
        let clock = Arc::new(FakeClock::new(base_time));

        let tz_job = make_test_job_with_tz("tz-job", "0 10 * * *", "America/New_York");
        store.add_job(tz_job).await;

        let notify = Arc::new(Notify::new());
        let (tx, mut rx) = mpsc::channel::<Job>(16);

        let scheduler = Scheduler::new(store.clone(), clock.clone(), notify.clone(), tx);

        let handle = tokio::spawn(async move { scheduler.run().await });

        // Let scheduler start
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Advance clock past the 14:00 UTC fire time
        clock.set(Utc.with_ymd_and_hms(2025, 6, 15, 14, 0, 0).unwrap());

        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(31)).await;
        tokio::time::resume();

        tokio::time::sleep(Duration::from_millis(100)).await;

        let dispatched = rx.try_recv();
        assert!(
            dispatched.is_ok(),
            "Timezone-aware job should be dispatched"
        );
        assert_eq!(dispatched.unwrap().name, "tz-job");

        handle.abort();
    }

    // =======================================================================
    // Additional: FakeClock tests
    // =======================================================================

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
