use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use indexmap::IndexMap;
use tokio::sync::{broadcast, watch, RwLock};
use uuid::Uuid;

use crate::daemon::events::WorkflowEvent;
use crate::models::workflow::{
    FailurePolicy, RunStatus, StepDef, StepRun, TriggerParams, Workflow, WorkflowRun,
};
use crate::workflow::step::{KillSender, LogSink, Step, StepContext, StepError, StepOutput};
use crate::workflow::template;

/// Helper: send a `WorkflowEvent` on the optional broadcast channel.
/// Failures (no receivers) are silently ignored.
fn emit(tx: Option<&broadcast::Sender<WorkflowEvent>>, event: WorkflowEvent) {
    if let Some(tx) = tx {
        let _ = tx.send(event);
    }
}

// ── Step dispatch helper ──────────────────────────────────────────────────────

/// Dispatch a single `StepDef` to the correct `Step::execute` call.
///
/// Unimplemented variants return `StepError::Internal` with a phase note.
async fn dispatch_step(step_def: &StepDef, ctx: &mut StepContext) -> Result<StepOutput, StepError> {
    match step_def {
        StepDef::Shell(s) => s.execute(ctx).await,
        StepDef::Script(s) => s.execute(ctx).await,
        StepDef::SetVar(s) => s.execute(ctx).await,
        StepDef::Http(s) => s.execute(ctx).await,
        StepDef::Agent(s) => s.execute(ctx).await,
        // MatchStep is handled directly in execute_steps; this arm is unreachable.
        StepDef::Match(_) => Err(StepError::Internal(
            "MatchStep dispatched directly in execute_steps".to_string(),
        )),
    }
}

/// Extract the `StepDefCommon` ref from any `StepDef` variant.
fn step_common(def: &StepDef) -> &crate::models::workflow::StepDefCommon {
    match def {
        StepDef::Shell(s) => &s.common,
        StepDef::Script(s) => &s.common,
        StepDef::SetVar(s) => &s.common,
        StepDef::Http(s) => &s.common,
        StepDef::Agent(s) => &s.common,
        StepDef::Match(s) => &s.common,
    }
}

// ── Core step-sequence runner ─────────────────────────────────────────────────

/// Walk `steps` in order, executing each or skipping based on `aborted`.
///
/// Skipped steps (post-abort, non-always_run) are NOT recorded in `step_runs`.
/// Steps after a `StepError::Killed` are also omitted from `step_runs`.
///
/// On Abort policy failure, `*aborted` is set to true and the loop continues
/// so that `always_run` cleanup steps can still execute.
///
/// Returns `true` if a `StepError::Killed` was encountered (the caller should
/// set the final `RunStatus` to `Killed`).
async fn execute_steps(
    steps: &[StepDef],
    workflow: &Workflow,
    ctx: &mut StepContext,
    step_runs: &mut Vec<StepRun>,
    aborted: &mut bool,
    killed: &mut bool,
) {
    for step_def in steps {
        let common = step_common(step_def);

        // Determine whether this step should execute.
        let should_run = if *aborted || *killed {
            common.always_run
        } else {
            true
        };

        if !should_run {
            // Skipped steps are omitted from step_runs.
            // Skipped steps do NOT emit StepStarted / StepCompleted events.
            continue;
        }

        // step_index is 0-based across the entire run. The first executed
        // step gets index 0, the second gets 1, etc. `ctx.step_index` is
        // assigned (not pre-incremented) so the value is correct for the
        // current step; the post-increment prepares the counter for the
        // next call.
        let step_index = ctx.step_index;
        ctx.step_index = step_index + 1;

        // ── MatchStep: special handling ───────────────────────────────────────

        if let StepDef::Match(m) = step_def {
            let run_id = ctx.run_id;
            let workflow_id = ctx.workflow_id;

            // Emit StepStarted for the synthetic match step.
            emit(
                ctx.event_tx.as_ref(),
                WorkflowEvent::StepStarted {
                    run_id,
                    workflow_id,
                    step_index,
                    step_id: m.common.id.clone(),
                    kind: "match".to_string(),
                    started_at: Utc::now(),
                },
            );
            tracing::info!(
                run_id = %run_id,
                step_index = step_index,
                step_id = %m.common.id,
                kind = "match",
                "step started"
            );

            let sub = template::substitute(&m.expr, &ctx.input, &ctx.steps);
            for warn in &sub.warnings {
                tracing::warn!(step_id = %m.common.id, "match expr warning: {}", warn);
            }
            let evaluated = sub.output;

            // Determine which branch (if any) to recurse into.
            let (branch_steps, case_taken): (Option<&Vec<StepDef>>, &str) = {
                if let Some(branch) = m.cases.get(&evaluated) {
                    (Some(branch), evaluated.as_str())
                } else if let Some(ref default) = m.default {
                    (Some(default), "default")
                } else {
                    (None, "none")
                }
            };

            let started_at = Utc::now();

            // Synthetic StepRun for the MatchStep itself. The case_taken
            // decision is recorded in the in-memory StepOutput below (and
            // flows into ${steps.<id>.exports.*} for downstream substitution)
            // but is not persisted on the StepRun — that record carries the
            // log byte range plus structural fields only.
            tracing::debug!(
                step_id = %m.common.id,
                evaluated = %evaluated,
                case_taken = %case_taken,
                "match step decision"
            );
            // The match step itself does NOT write to the log, but the child
            // steps in the chosen branch do. Record offsets that span the
            // first-child-START through last-child-END so a slice query on
            // the match step returns the branch's bytes.
            //
            // We push the synthetic match_run BEFORE the branch executes (so
            // step_runs is in execution order), then patch its
            // `log_byte_offset_start` / `_end` AFTER the branch finishes by
            // tracking the first / last child step that was pushed during
            // the recursion.
            let match_run = StepRun {
                step_index,
                step_id: m.common.id.clone(),
                kind: "match".to_string(),
                status: RunStatus::Completed,
                started_at,
                finished_at: Some(Utc::now()),
                exit_code: Some(0),
                log_byte_offset_start: 0,
                log_byte_offset_end: None,
                cost_usd: None,
                error: None,
            };

            // Emit StepCompleted for the match step.
            emit(
                ctx.event_tx.as_ref(),
                WorkflowEvent::StepCompleted {
                    run_id,
                    workflow_id,
                    step_index,
                    step_id: m.common.id.clone(),
                    exit_code: Some(0),
                    cost_usd: None,
                    finished_at: Utc::now(),
                },
            );
            tracing::info!(
                run_id = %run_id,
                step_index = step_index,
                step_id = %m.common.id,
                status = ?RunStatus::Completed,
                exit_code = 0,
                "step completed"
            );

            let match_run_idx = step_runs.len();
            step_runs.push(match_run);

            // Insert a placeholder so ${steps.<id>.*} can resolve.
            ctx.steps.insert(
                m.common.id.clone(),
                StepOutput {
                    exit_code: Some(0),
                    stdout: None,
                    exports: HashMap::new(),
                    cost: None,
                    log_byte_offset_start: None,
                    log_byte_offset_end: None,
                },
            );

            // Recurse into branch steps if a branch was matched.
            if let Some(branch) = branch_steps {
                // We need to clone to avoid borrow issues since execute_steps is recursive
                let branch_owned: Vec<StepDef> = branch.clone();
                let pre_len = step_runs.len();
                Box::pin(execute_steps(
                    &branch_owned,
                    workflow,
                    ctx,
                    step_runs,
                    aborted,
                    killed,
                ))
                .await;
                let post_len = step_runs.len();

                // Patch the match_run's byte offsets to span the children that
                // were just pushed (if any). The first child's _start becomes
                // the match step's _start; the last child's _end (or None if
                // it never finished) becomes the match step's _end.
                if post_len > pre_len {
                    let first_start = step_runs[pre_len].log_byte_offset_start;
                    let last_end = step_runs[post_len - 1].log_byte_offset_end;
                    if let Some(slot) = step_runs.get_mut(match_run_idx) {
                        slot.log_byte_offset_start = first_start;
                        slot.log_byte_offset_end = last_end;
                    }
                }
                // If no children ran (branch_steps was empty or all skipped),
                // the match step has no log slice — leave offsets as the
                // defaults (0 / None), which the log endpoint serves as
                // "tail to EOF".
            }

            continue;
        }

        // ── Regular step execution ────────────────────────────────────────────

        let run_id = ctx.run_id;
        let workflow_id = ctx.workflow_id;
        let step_kind = step_kind_str(step_def);

        // Emit StepStarted before executing the step.
        emit(
            ctx.event_tx.as_ref(),
            WorkflowEvent::StepStarted {
                run_id,
                workflow_id,
                step_index,
                step_id: common.id.clone(),
                kind: step_kind.to_string(),
                started_at: Utc::now(),
            },
        );
        tracing::info!(
            run_id = %run_id,
            step_index = step_index,
            step_id = %common.id,
            kind = %step_kind,
            "step started"
        );

        // Inform the log sink which step is active so that chunk events carry
        // the correct step_index and step_id.  Errors are logged but do not
        // abort the step — the chunk path is best-effort.
        if let Err(e) = ctx.log_sink.set_current_step(step_index, &common.id).await {
            tracing::warn!(
                step_id = %common.id,
                "log_sink.set_current_step failed (non-fatal): {}",
                e
            );
        }

        let started_at = Utc::now();
        let effective_policy = common
            .on_failure
            .clone()
            .unwrap_or_else(|| workflow.on_failure.clone());

        let result = run_step_with_policy(step_def, ctx, effective_policy, started_at).await;

        match result {
            StepRunResult::Completed(mut run, output) => {
                run.step_index = step_index;
                run.kind = step_kind.to_string();
                // Emit StepCompleted after successful execution.
                emit(
                    ctx.event_tx.as_ref(),
                    WorkflowEvent::StepCompleted {
                        run_id,
                        workflow_id,
                        step_index,
                        step_id: common.id.clone(),
                        exit_code: run.exit_code,
                        cost_usd: run.cost_usd,
                        finished_at: Utc::now(),
                    },
                );
                let duration_ms = run
                    .finished_at
                    .map(|f| (f - run.started_at).num_milliseconds().max(0) as u64);
                tracing::info!(
                    run_id = %run_id,
                    step_index = step_index,
                    step_id = %common.id,
                    status = ?run.status,
                    exit_code = ?run.exit_code,
                    duration_ms = ?duration_ms,
                    "step completed"
                );
                ctx.steps.insert(common.id.clone(), output);
                step_runs.push(run);
            }
            StepRunResult::Failed(mut run) => {
                run.step_index = step_index;
                run.kind = step_kind.to_string();
                // Emit StepCompleted with non-zero exit code or None.
                emit(
                    ctx.event_tx.as_ref(),
                    WorkflowEvent::StepCompleted {
                        run_id,
                        workflow_id,
                        step_index,
                        step_id: common.id.clone(),
                        exit_code: run.exit_code,
                        cost_usd: run.cost_usd,
                        finished_at: Utc::now(),
                    },
                );
                let duration_ms = run
                    .finished_at
                    .map(|f| (f - run.started_at).num_milliseconds().max(0) as u64);
                tracing::info!(
                    run_id = %run_id,
                    step_index = step_index,
                    step_id = %common.id,
                    status = ?run.status,
                    exit_code = ?run.exit_code,
                    duration_ms = ?duration_ms,
                    "step completed"
                );
                step_runs.push(run);
                *aborted = true;
            }
            StepRunResult::FailedContinue(mut run, output) => {
                run.step_index = step_index;
                run.kind = step_kind.to_string();
                // Insert the actual output (even though the step failed) so that
                // downstream template references like ${steps.<id>.exit_code} resolve.
                emit(
                    ctx.event_tx.as_ref(),
                    WorkflowEvent::StepCompleted {
                        run_id,
                        workflow_id,
                        step_index,
                        step_id: common.id.clone(),
                        exit_code: run.exit_code,
                        cost_usd: run.cost_usd,
                        finished_at: Utc::now(),
                    },
                );
                let duration_ms = run
                    .finished_at
                    .map(|f| (f - run.started_at).num_milliseconds().max(0) as u64);
                tracing::info!(
                    run_id = %run_id,
                    step_index = step_index,
                    step_id = %common.id,
                    status = ?run.status,
                    exit_code = ?run.exit_code,
                    duration_ms = ?duration_ms,
                    "step completed"
                );
                ctx.steps.insert(common.id.clone(), output);
                step_runs.push(run);
                // Do NOT set aborted — continue policy means keep going.
            }
            StepRunResult::Killed(mut run) => {
                run.step_index = step_index;
                run.kind = step_kind.to_string();
                emit(
                    ctx.event_tx.as_ref(),
                    WorkflowEvent::StepCompleted {
                        run_id,
                        workflow_id,
                        step_index,
                        step_id: common.id.clone(),
                        exit_code: run.exit_code,
                        cost_usd: run.cost_usd,
                        finished_at: Utc::now(),
                    },
                );
                let duration_ms = run
                    .finished_at
                    .map(|f| (f - run.started_at).num_milliseconds().max(0) as u64);
                tracing::info!(
                    run_id = %run_id,
                    step_index = step_index,
                    step_id = %common.id,
                    status = ?run.status,
                    exit_code = ?run.exit_code,
                    duration_ms = ?duration_ms,
                    "step completed"
                );
                step_runs.push(run);
                *killed = true;
                *aborted = true; // stop further steps
            }
        }
    }
}

/// Derive the step kind string from a `StepDef`.
fn step_kind_str(def: &StepDef) -> &'static str {
    match def {
        StepDef::Shell(_) => "shell",
        StepDef::Script(_) => "script",
        StepDef::Http(_) => "http",
        StepDef::Match(_) => "match",
        StepDef::SetVar(_) => "set_var",
        StepDef::Agent(_) => "agent",
    }
}

/// Outcome of executing a single step (with retry logic applied).
enum StepRunResult {
    /// Step completed successfully.
    Completed(StepRun, StepOutput),
    /// Step failed with Abort policy (or all retries exhausted → treated as Abort).
    Failed(StepRun),
    /// Step failed with Continue policy — carries the output for ctx.steps insertion.
    FailedContinue(StepRun, StepOutput),
    /// Step was killed.
    Killed(StepRun),
}

/// Run a step, applying retry logic, and return a `StepRunResult`.
async fn run_step_with_policy(
    step_def: &StepDef,
    ctx: &mut StepContext,
    policy: FailurePolicy,
    started_at: chrono::DateTime<Utc>,
) -> StepRunResult {
    let common = step_common(step_def);

    match policy {
        FailurePolicy::Abort | FailurePolicy::Continue => {
            let result = dispatch_step(step_def, ctx).await;
            build_step_run_result(common, result, policy, started_at)
        }
        FailurePolicy::Retry {
            attempts,
            backoff_ms,
        } => {
            // NOTE: per-attempt StepRun rows are not recorded; only the final outcome is.
            // This keeps the step_runs table clean. A future phase may add per-attempt rows.
            let max_attempts = attempts.max(1);
            let mut last_err: Option<StepError> = None;

            for attempt in 0..max_attempts {
                if attempt > 0 && backoff_ms > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(backoff_ms)).await;
                }

                match dispatch_step(step_def, ctx).await {
                    Ok(output) => {
                        let is_failure = output.exit_code.map(|c| c != 0).unwrap_or(false);
                        if is_failure {
                            // Non-zero exit — store a synthetic error and retry.
                            last_err = Some(StepError::Internal(format!(
                                "step exited with non-zero code: {}",
                                output.exit_code.unwrap_or(-1)
                            )));
                        } else {
                            let run = make_step_run(
                                common,
                                started_at,
                                RunStatus::Completed,
                                &output,
                                None,
                            );
                            return StepRunResult::Completed(run, output);
                        }
                    }
                    Err(StepError::Killed) => {
                        // Kill is always terminal regardless of policy.
                        let run = make_failed_step_run(common, started_at, "kill requested");
                        return StepRunResult::Killed(run);
                    }
                    Err(e) => {
                        last_err = Some(e);
                    }
                }
            }

            // All retries exhausted — treat as Abort.
            let err_msg = last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "unknown error".to_string());
            let run = make_failed_step_run(common, started_at, &err_msg);
            StepRunResult::Failed(run)
        }
    }
}

fn build_step_run_result(
    common: &crate::models::workflow::StepDefCommon,
    result: Result<StepOutput, StepError>,
    policy: FailurePolicy,
    started_at: chrono::DateTime<Utc>,
) -> StepRunResult {
    match result {
        Ok(output) => {
            // A non-zero exit code is a step failure — apply the failure policy.
            let is_failure = output.exit_code.map(|c| c != 0).unwrap_or(false);
            if is_failure {
                let err_msg = format!(
                    "step exited with non-zero code: {}",
                    output.exit_code.unwrap_or(-1)
                );
                let run = make_step_run(
                    common,
                    started_at,
                    RunStatus::Failed,
                    &output,
                    Some(err_msg),
                );
                match policy {
                    FailurePolicy::Continue => {
                        // On Continue, carry the real output so downstream templates can
                        // reference ${steps.<id>.exit_code} etc.
                        StepRunResult::FailedContinue(run, output)
                    }
                    _ => StepRunResult::Failed(run),
                }
            } else {
                let run = make_step_run(common, started_at, RunStatus::Completed, &output, None);
                StepRunResult::Completed(run, output)
            }
        }
        Err(StepError::Killed) => {
            let run = make_failed_step_run(common, started_at, "kill requested");
            StepRunResult::Killed(run)
        }
        Err(e) => {
            let err_msg = e.to_string();
            let run = make_failed_step_run(common, started_at, &err_msg);
            match policy {
                FailurePolicy::Continue => {
                    // No output from an Err variant — use a placeholder.
                    let placeholder = StepOutput {
                        exit_code: None,
                        stdout: None,
                        exports: HashMap::new(),
                        cost: None,
                        log_byte_offset_start: None,
                        log_byte_offset_end: None,
                    };
                    StepRunResult::FailedContinue(run, placeholder)
                }
                _ => StepRunResult::Failed(run),
            }
        }
    }
}

fn make_step_run(
    common: &crate::models::workflow::StepDefCommon,
    started_at: chrono::DateTime<Utc>,
    status: RunStatus,
    output: &StepOutput,
    error: Option<String>,
) -> StepRun {
    StepRun {
        step_index: 0, // patched at call site in execute_steps via mut run.step_index
        step_id: common.id.clone(),
        kind: "unknown".to_string(), // patched at call site
        status,
        started_at,
        finished_at: Some(Utc::now()),
        exit_code: output.exit_code,
        log_byte_offset_start: output.log_byte_offset_start.unwrap_or(0),
        log_byte_offset_end: output.log_byte_offset_end,
        cost_usd: output.cost.as_ref().and_then(|c| c.total_cost_usd),
        error,
    }
}

fn make_failed_step_run(
    common: &crate::models::workflow::StepDefCommon,
    started_at: chrono::DateTime<Utc>,
    error: &str,
) -> StepRun {
    // Err paths from step impls do not surface byte offsets (the step impl
    // already returned). We record `log_byte_offset_start = 0` and
    // `_end = None`; the GET /api/runs/{id}/log?step_index=N handler treats
    // `_end = None` as "tail to EOF", which gives a sensible best-effort slice
    // for a failed/killed step (it returns everything from the very start of
    // the log, but at least the endpoint still serves bytes).
    StepRun {
        step_index: 0, // patched at call site in execute_steps via mut run.step_index
        step_id: common.id.clone(),
        kind: "unknown".to_string(), // patched at call site
        status: RunStatus::Failed,
        started_at,
        finished_at: Some(Utc::now()),
        exit_code: None,
        log_byte_offset_start: 0,
        log_byte_offset_end: None,
        cost_usd: None,
        error: Some(error.to_string()),
    }
}

/// Derive the step kind string from the step definition (legacy stub).
#[allow(dead_code)]
fn kind_from_common(_common: &crate::models::workflow::StepDefCommon) -> String {
    "unknown".to_string()
}

// ── Public entry point ────────────────────────────────────────────────────────

/// Run a workflow to completion, returning a fully-populated [`WorkflowRun`].
///
/// `run_id` should be pre-generated by the dispatcher (e.g., scheduler) for
/// SSE event correlation.
///
/// `trigger` carries the initial input and any env overlay.
/// `log_sink` receives structured step markers and raw output chunks.
/// `event_tx` is an optional broadcast channel for `WorkflowEvent` SSE streaming.
///   Pass `None` in tests or contexts that don't require live events.
/// `kill_signals` is an optional registry of kill senders keyed by run_id.
///   When provided, a new `watch::channel(false)` is inserted at the start and
///   removed when the run completes.  Pass `None` in tests or contexts that
///   don't require kill support.
///
/// Note: `StepOutput` chunk events (`WorkflowEvent::StepOutput`) are deferred to
/// phase 6, where they will be wired into the log sink's streaming path.
pub async fn run_workflow(
    workflow: &Workflow,
    run_id: Uuid,
    trigger: TriggerParams,
    log_sink: Arc<dyn LogSink>,
    event_tx: Option<broadcast::Sender<WorkflowEvent>>,
    kill_signals: Option<Arc<RwLock<HashMap<Uuid, KillSender>>>>,
) -> WorkflowRun {
    let snapshot = workflow.clone();
    let started_at = Utc::now();

    // Create kill channel for this run.  The sender is stored in the registry
    // (if provided) so the kill endpoint can signal it later.
    let (kill_tx, kill_rx) = watch::channel(false);
    if let Some(ref registry) = kill_signals {
        registry.write().await.insert(run_id, kill_tx);
    }

    // Emit RunStarted before executing any steps.
    emit(
        event_tx.as_ref(),
        WorkflowEvent::RunStarted {
            run_id,
            workflow_id: workflow.id,
            workflow_version: workflow.version,
            started_at,
        },
    );
    tracing::info!(
        run_id = %run_id,
        workflow_id = %workflow.id,
        name = %workflow.name,
        "run started"
    );

    // Resolve effective input: trigger.input if not Null, else workflow.default_input.
    let effective_input = if trigger.input.is_null() {
        workflow
            .default_input
            .clone()
            .unwrap_or(serde_json::Value::Null)
    } else {
        trigger.input.clone()
    };

    // Merge env: workflow base, then trigger overlay (trigger wins on conflicts).
    let mut env: HashMap<String, String> = workflow.env_vars.clone().unwrap_or_default();
    if let Some(trigger_env) = trigger.env {
        env.extend(trigger_env);
    }

    let working_dir = workflow
        .working_dir
        .as_deref()
        .map(std::path::PathBuf::from);

    let mut ctx = StepContext {
        workflow_id: workflow.id,
        workflow_version: workflow.version,
        run_id,
        step_index: 0,
        input: effective_input.clone(),
        steps: IndexMap::new(),
        log_sink,
        working_dir,
        env,
        event_tx: event_tx.clone(),
        kill_rx: Some(kill_rx),
        target_step: trigger.target_step.clone(),
    };

    let mut step_runs: Vec<StepRun> = Vec::new();
    let mut aborted = false;
    let mut killed = false;

    // Clone the steps to avoid holding a borrow on `workflow` while `ctx` borrows it
    let steps_owned: Vec<StepDef> = workflow.steps.clone();
    execute_steps(
        &steps_owned,
        workflow,
        &mut ctx,
        &mut step_runs,
        &mut aborted,
        &mut killed,
    )
    .await;

    let finished_at = Utc::now();

    // Remove the kill sender from the registry now that the run has finished.
    // This also drops the sender, which signals any remaining receivers that
    // no kill will arrive (they receive a RecvError on next `changed()` call).
    if let Some(ref registry) = kill_signals {
        registry.write().await.remove(&run_id);
    }

    // Determine final status.
    let status = if killed {
        RunStatus::Killed
    } else if aborted {
        RunStatus::Failed
    } else {
        RunStatus::Completed
    };

    // Sum total cost (None counts as 0; if no step has cost, result is None).
    let total_cost_usd = {
        let any_cost = step_runs.iter().any(|r| r.cost_usd.is_some());
        if any_cost {
            Some(step_runs.iter().map(|r| r.cost_usd.unwrap_or(0.0)).sum())
        } else {
            None
        }
    };

    let total_duration_ms = (finished_at - started_at).num_milliseconds().max(0) as u64;

    // Emit RunCompleted or RunFailed.
    match status {
        RunStatus::Completed => {
            emit(
                event_tx.as_ref(),
                WorkflowEvent::RunCompleted {
                    run_id,
                    workflow_id: workflow.id,
                    status: RunStatus::Completed,
                    total_cost_usd,
                    finished_at,
                },
            );
            tracing::info!(
                run_id = %run_id,
                status = "Completed",
                total_duration_ms = total_duration_ms,
                total_cost_usd = ?total_cost_usd,
                "run finished"
            );
        }
        RunStatus::Failed | RunStatus::Killed => {
            // Use the first error from step_runs as the failure message.
            let error_msg = step_runs
                .iter()
                .find_map(|r| r.error.as_deref())
                .unwrap_or("workflow run failed")
                .to_string();
            emit(
                event_tx.as_ref(),
                WorkflowEvent::RunFailed {
                    run_id,
                    workflow_id: workflow.id,
                    error: error_msg,
                    finished_at,
                },
            );
            tracing::info!(
                run_id = %run_id,
                status = ?status,
                total_duration_ms = total_duration_ms,
                total_cost_usd = ?total_cost_usd,
                "run finished"
            );
        }
        RunStatus::Running => {
            // Should not happen at end of run_workflow; emit RunFailed defensively.
            emit(
                event_tx.as_ref(),
                WorkflowEvent::RunFailed {
                    run_id,
                    workflow_id: workflow.id,
                    error: "unexpected Running status at end of run_workflow".to_string(),
                    finished_at,
                },
            );
        }
    }

    WorkflowRun {
        run_id,
        workflow_id: workflow.id,
        workflow_version: workflow.version,
        workflow_snapshot: snapshot,
        started_at,
        finished_at: Some(finished_at),
        status,
        trigger_input: if effective_input.is_null() {
            None
        } else {
            Some(effective_input)
        },
        steps: step_runs,
        total_cost_usd,
        total_duration_ms: Some(total_duration_ms),
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::{json, Value};
    use uuid::Uuid;

    use crate::models::workflow::ScheduleMode;
    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, MatchStep, RunStatus, SetVarStep, ShellStep, StepDef,
        StepDefCommon, TriggerParams, Workflow,
    };
    use crate::workflow::step::LogSink;

    use super::run_workflow;

    // ── Mock LogSink ──────────────────────────────────────────────────────────

    #[derive(Clone, Default)]
    struct MockLogSink {
        chunks: Arc<Mutex<Vec<u8>>>,
        events: Arc<Mutex<Vec<String>>>,
        /// Running byte position — start/end markers each advance it by a
        /// fixed sentinel amount so tests can assert monotonic offsets.
        pos: Arc<Mutex<u64>>,
    }

    #[async_trait]
    impl LogSink for MockLogSink {
        async fn write_step_start(
            &self,
            step_id: &str,
            _started_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events
                .lock()
                .unwrap()
                .push(format!("start:{}", step_id));
            let mut pos = self.pos.lock().unwrap();
            let before = *pos;
            *pos += 32; // sentinel marker size
            Ok(before)
        }

        async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
            self.chunks.lock().unwrap().extend_from_slice(data);
            *self.pos.lock().unwrap() += data.len() as u64;
            Ok(())
        }

        async fn write_step_end(
            &self,
            step_id: &str,
            exit_code: Option<i32>,
            _finished_at: DateTime<Utc>,
        ) -> std::io::Result<u64> {
            self.events.lock().unwrap().push(format!(
                "end:{}:{}",
                step_id,
                exit_code.map(|c| c.to_string()).unwrap_or("-1".to_string())
            ));
            let mut pos = self.pos.lock().unwrap();
            *pos += 32; // sentinel marker size
            Ok(*pos)
        }
    }

    // ── Workflow builder helpers ───────────────────────────────────────────────

    fn now() -> DateTime<Utc> {
        Utc::now()
    }

    fn make_workflow(id: &str, steps: Vec<StepDef>) -> Workflow {
        make_workflow_with_policy(id, steps, FailurePolicy::Abort)
    }

    fn make_workflow_with_policy(id: &str, steps: Vec<StepDef>, policy: FailurePolicy) -> Workflow {
        let n = now();
        Workflow {
            id: Uuid::now_v7(),
            name: id.to_string(),
            version: 1,
            schedule: "* * * * *".to_string(),
            timezone: None,
            schedule_mode: ScheduleMode::default(),
            enabled: true,
            steps,
            default_input: None,
            working_dir: None,
            env_vars: None,
            allow_concurrent: true,
            on_failure: policy,
            last_run_at: None,
            last_run_status: None,
            last_run_id: None,
            next_run_at: None,
            created_at: n,
            updated_at: n,
        }
    }

    fn shell_step(id: &str, cmd: &str) -> StepDef {
        StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: cmd.to_string(),
            pass_stdin: false,
        })
    }

    fn shell_step_with_policy(id: &str, cmd: &str, policy: FailurePolicy) -> StepDef {
        StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: Some(policy),
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: cmd.to_string(),
            pass_stdin: false,
        })
    }

    fn shell_step_always_run(id: &str, cmd: &str) -> StepDef {
        StepDef::Shell(ShellStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: true,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            command: cmd.to_string(),
            pass_stdin: false,
        })
    }

    fn set_var_step(id: &str, exports: HashMap<String, String>) -> StepDef {
        StepDef::SetVar(SetVarStep {
            common: StepDefCommon {
                id: id.to_string(),
                on_failure: None,
                always_run: false,
                timeout_secs: None,
                working_dir: None,
                env_vars: None,
                capture: CaptureSpec::default(),
            },
            exports,
        })
    }

    fn empty_trigger() -> TriggerParams {
        TriggerParams {
            input: Value::Null,
            env: None,
            target_step: None,
        }
    }

    fn input_trigger(input: Value) -> TriggerParams {
        TriggerParams {
            input,
            env: None,
            target_step: None,
        }
    }

    /// Platform-appropriate "exit 1" command.
    fn exit_one_cmd() -> &'static str {
        #[cfg(windows)]
        {
            "exit 1"
        }
        #[cfg(not(windows))]
        {
            "sh -c 'exit 1'"
        }
    }

    // ── Test 1: 3-step happy path ──────────────────────────────────────────────

    #[tokio::test]
    async fn test_executor_three_step_happy_path() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "happy",
            vec![
                shell_step("s1", "echo a"),
                shell_step("s2", "echo b"),
                shell_step("s3", "echo c"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed, "expected Completed");
        assert_eq!(run.steps.len(), 3, "expected 3 step runs");
        for step_run in &run.steps {
            assert_eq!(
                step_run.exit_code,
                Some(0),
                "step {} should exit 0",
                step_run.step_id
            );
            assert_eq!(step_run.status, RunStatus::Completed);
        }
    }

    // ── Test 2: Abort on failure ───────────────────────────────────────────────

    #[tokio::test]
    async fn test_executor_abort_on_failure_skips_subsequent() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "abort",
            vec![
                shell_step_with_policy("fail", exit_one_cmd(), FailurePolicy::Abort),
                shell_step("never", "echo never"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Failed, "expected Failed");
        // The failing step is recorded; the skipped step is NOT recorded.
        assert_eq!(
            run.steps.len(),
            1,
            "only the failed step should be recorded"
        );
        assert_eq!(run.steps[0].step_id, "fail");
        assert_eq!(run.steps[0].status, RunStatus::Failed);
    }

    // ── Test 3: always_run cleanup after Abort ────────────────────────────────

    #[tokio::test]
    async fn test_executor_always_run_cleanup_after_abort() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "cleanup",
            vec![
                shell_step_with_policy("fail", exit_one_cmd(), FailurePolicy::Abort),
                shell_step_always_run("cleanup", "echo cleanup"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        // Overall status is Failed (aborted=true), but cleanup ran.
        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.steps.len(), 2, "both steps should be recorded");

        let cleanup = run.steps.iter().find(|r| r.step_id == "cleanup").unwrap();
        assert_eq!(cleanup.status, RunStatus::Completed);
    }

    // ── Test 4: Continue policy ────────────────────────────────────────────────

    #[tokio::test]
    async fn test_executor_continue_policy_runs_subsequent() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "continue_pol",
            vec![
                shell_step_with_policy("fail", exit_one_cmd(), FailurePolicy::Continue),
                shell_step("after", "echo after"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        // No abort happened, so overall Completed.
        assert_eq!(
            run.status,
            RunStatus::Completed,
            "expected Completed despite step failure"
        );
        assert_eq!(run.steps.len(), 2);

        let first = run.steps.iter().find(|r| r.step_id == "fail").unwrap();
        assert_eq!(first.status, RunStatus::Failed);

        let second = run.steps.iter().find(|r| r.step_id == "after").unwrap();
        assert_eq!(second.status, RunStatus::Completed);
    }

    // ── Test 5: Retry (TODO) ──────────────────────────────────────────────────
    // TODO retry test in phase 3.1 — reliably producing a "fail then succeed"
    // with Shell steps requires persistent state between retries which is tricky.

    // ── Test 6: MatchStep happy path ──────────────────────────────────────────

    #[tokio::test]
    async fn test_executor_match_step_happy_path() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        // set_choice exports {"choice": "\"A\""} → after JSON parse → Value::String("A")
        let mut sv_exports = HashMap::new();
        sv_exports.insert("choice".to_string(), r#""A""#.to_string());

        let mut cases = HashMap::new();
        cases.insert("A".to_string(), vec![shell_step("branch_a", "echo A path")]);
        cases.insert("B".to_string(), vec![shell_step("branch_b", "echo B path")]);

        let workflow = make_workflow(
            "match_happy",
            vec![
                set_var_step("set_choice", sv_exports),
                StepDef::Match(MatchStep {
                    common: StepDefCommon {
                        id: "m1".to_string(),
                        on_failure: None,
                        always_run: false,
                        timeout_secs: None,
                        working_dir: None,
                        env_vars: None,
                        capture: CaptureSpec::default(),
                    },
                    expr: "${steps.set_choice.exports.choice}".to_string(),
                    cases,
                    default: None,
                }),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        // Expected step_runs: set_choice, m1 (synthetic), branch_a
        assert_eq!(
            run.steps.len(),
            3,
            "expected 3 step runs; got: {:?}",
            run.steps.iter().map(|r| &r.step_id).collect::<Vec<_>>()
        );

        let match_run = run.steps.iter().find(|r| r.step_id == "m1").unwrap();
        assert_eq!(match_run.status, RunStatus::Completed);
        // The "A" case was taken: branch_a ran and branch_b did not.
        let branch_run = run.steps.iter().find(|r| r.step_id == "branch_a").unwrap();
        assert_eq!(branch_run.status, RunStatus::Completed);
        assert!(
            !run.steps.iter().any(|r| r.step_id == "branch_b"),
            "branch_b must not run when case 'A' is selected"
        );
    }

    // ── Test 7: MatchStep no case + default branch ────────────────────────────

    #[tokio::test]
    async fn test_executor_match_step_default_branch() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let mut sv_exports = HashMap::new();
        sv_exports.insert("val".to_string(), "unknown".to_string());

        let mut cases = HashMap::new();
        cases.insert("A".to_string(), vec![shell_step("branch_a2", "echo A")]);

        let default_steps = vec![shell_step("default_step", "echo default")];

        let workflow = make_workflow(
            "match_default",
            vec![
                set_var_step("set_val", sv_exports),
                StepDef::Match(MatchStep {
                    common: StepDefCommon {
                        id: "m2".to_string(),
                        on_failure: None,
                        always_run: false,
                        timeout_secs: None,
                        working_dir: None,
                        env_vars: None,
                        capture: CaptureSpec::default(),
                    },
                    expr: "${steps.set_val.exports.val}".to_string(),
                    cases,
                    default: Some(default_steps),
                }),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);

        let match_run = run.steps.iter().find(|r| r.step_id == "m2").unwrap();
        assert_eq!(match_run.status, RunStatus::Completed);

        assert!(
            run.steps.iter().any(|r| r.step_id == "default_step"),
            "default branch step should appear in step_runs"
        );
        assert!(
            !run.steps.iter().any(|r| r.step_id == "branch_a2"),
            "no case branch should have run"
        );
    }

    // ── Test 8: MatchStep no case + no default → no-op ────────────────────────

    #[tokio::test]
    async fn test_executor_match_step_no_match_no_default_noop() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let mut sv_exports = HashMap::new();
        sv_exports.insert("val".to_string(), "Z".to_string());

        let mut cases = HashMap::new();
        cases.insert("A".to_string(), vec![shell_step("branch_a3", "echo A")]);

        let workflow = make_workflow(
            "match_noop",
            vec![
                set_var_step("set_val2", sv_exports),
                StepDef::Match(MatchStep {
                    common: StepDefCommon {
                        id: "m3".to_string(),
                        on_failure: None,
                        always_run: false,
                        timeout_secs: None,
                        working_dir: None,
                        env_vars: None,
                        capture: CaptureSpec::default(),
                    },
                    expr: "${steps.set_val2.exports.val}".to_string(),
                    cases,
                    default: None,
                }),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);

        let match_run = run.steps.iter().find(|r| r.step_id == "m3").unwrap();
        assert_eq!(match_run.status, RunStatus::Completed);

        // No branch steps should appear
        assert!(
            !run.steps.iter().any(|r| r.step_id == "branch_a3"),
            "no branch steps should appear in step_runs for a no-op match"
        );
    }

    // ── Test 9: HttpStep is now dispatched (phase 4) — unreachable host fails ──

    #[tokio::test]
    async fn test_executor_http_step_dispatched() {
        use crate::models::workflow::HttpStep;

        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        // Port 1 is almost always closed/refused; we just want to confirm the
        // Http arm is dispatched (not returning "not implemented in phase 3").
        let workflow = make_workflow(
            "http_dispatched",
            vec![StepDef::Http(HttpStep {
                common: StepDefCommon {
                    id: "http1".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: Some(2),
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                method: "GET".to_string(),
                url: "http://127.0.0.1:1/unreachable".to_string(),
                headers: HashMap::new(),
                body: None,
                expect_status: vec![200],
            })],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Failed);
        let step = &run.steps[0];
        assert_eq!(step.status, RunStatus::Failed);
        let err = step.error.as_ref().unwrap();
        // Must NOT contain the old phase-3 stub message
        assert!(
            !err.contains("not implemented in phase 3"),
            "HttpStep should be implemented now, got: {}",
            err
        );
    }

    // ── Test 10: AgentStep is now dispatched (phase 4) ────────────────────────
    // Verifies the agent step is actually invoked (not stubbed out with a
    // "not implemented" error). The terminal status depends on whether the
    // `claude` CLI happens to be on the test runner's PATH — both outcomes
    // are acceptable here; what matters is that the step dispatched.

    #[tokio::test]
    async fn test_executor_agent_step_dispatched() {
        use crate::models::workflow::{AgentStep, AgentType};

        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let workflow = make_workflow(
            "agent_dispatched",
            vec![StepDef::Agent(AgentStep {
                common: StepDefCommon {
                    id: "ag1".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                agent_type: AgentType::ClaudeCodeCli,
                prompt: "do something".to_string(),
                command_template: None,
            })],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        // Step ran (was dispatched), so there should be exactly 1 step run.
        assert_eq!(run.steps.len(), 1);
        // Whatever the outcome, it must NOT be the old "not implemented" stub.
        if let Some(err) = run.steps[0].error.as_ref() {
            assert!(
                !err.contains("not implemented in phase 3"),
                "AgentStep should no longer return a phase-3 not-implemented error, got: {}",
                err
            );
        }
        assert!(
            matches!(run.status, RunStatus::Completed | RunStatus::Failed),
            "expected Completed or Failed, got {:?}",
            run.status
        );
    }

    // ── Test 11: workflow.default_input applied when trigger.input is Null ────

    #[tokio::test]
    async fn test_executor_default_input_applied() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let mut workflow =
            make_workflow("default_input", vec![shell_step("s1", "echo ${input.x}")]);
        workflow.default_input = Some(json!({"x": 42}));

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        // The step should have run with x=42
        assert_eq!(run.steps.len(), 1);
        assert_eq!(run.steps[0].status, RunStatus::Completed);
        // Verify the trigger_input stored in the run matches default_input
        assert_eq!(run.trigger_input, Some(json!({"x": 42})));
    }

    // ── Test 12: trigger.input replaces default_input ─────────────────────────

    #[tokio::test]
    async fn test_executor_trigger_input_overrides_default() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let mut workflow =
            make_workflow("input_override", vec![shell_step("s1", "echo ${input.x}")]);
        workflow.default_input = Some(json!({"x": 42}));

        let trigger = input_trigger(json!({"x": 99}));
        let run = run_workflow(&workflow, Uuid::now_v7(), trigger, sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.trigger_input, Some(json!({"x": 99})));
    }

    // ── Test 13: trigger env overlays workflow env ─────────────────────────────

    #[cfg(unix)]
    #[tokio::test]
    async fn test_executor_trigger_env_overlays_workflow_env() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let mut workflow = make_workflow(
            "env_overlay",
            vec![shell_step("s1", "echo FOO=$FOO BAR=$BAR")],
        );
        let mut wf_env = HashMap::new();
        wf_env.insert("FOO".to_string(), "a".to_string());
        workflow.env_vars = Some(wf_env);

        let trigger = TriggerParams {
            input: Value::Null,
            env: Some({
                let mut e = HashMap::new();
                e.insert("FOO".to_string(), "b".to_string());
                e.insert("BAR".to_string(), "c".to_string());
                e
            }),
            target_step: None,
        };

        let run = run_workflow(
            &workflow,
            Uuid::now_v7(),
            trigger,
            Arc::clone(&sink) as Arc<dyn LogSink>,
            None,
            None,
        )
        .await;

        assert_eq!(run.status, RunStatus::Completed);
        // Verify through the mock log sink chunks that FOO=b and BAR=c appear
        if let Some(sink_inner) = Arc::downcast::<MockLogSink>(sink).ok() {
            let output = String::from_utf8_lossy(&sink_inner.chunks.lock().unwrap()).to_string();
            assert!(
                output.contains("FOO=b"),
                "expected FOO=b in output: {}",
                output
            );
            assert!(
                output.contains("BAR=c"),
                "expected BAR=c in output: {}",
                output
            );
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn test_executor_trigger_env_overlays_workflow_env() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let mut workflow = make_workflow(
            "env_overlay",
            vec![shell_step("s1", "echo FOO=%FOO% BAR=%BAR%")],
        );
        let mut wf_env = HashMap::new();
        wf_env.insert("FOO".to_string(), "a".to_string());
        workflow.env_vars = Some(wf_env);

        let trigger = TriggerParams {
            input: Value::Null,
            env: Some({
                let mut e = HashMap::new();
                e.insert("FOO".to_string(), "b".to_string());
                e.insert("BAR".to_string(), "c".to_string());
                e
            }),
            target_step: None,
        };

        let run = run_workflow(&workflow, Uuid::now_v7(), trigger, sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
    }

    // ── Test 14: WorkflowRun has correct metadata ──────────────────────────────

    #[tokio::test]
    async fn test_executor_run_metadata() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow("meta", vec![shell_step("s1", "echo hi")]);
        let run_id = Uuid::now_v7();

        let run = run_workflow(&workflow, run_id, empty_trigger(), sink, None, None).await;

        assert_eq!(run.run_id, run_id);
        assert_eq!(run.workflow_id, workflow.id);
        assert_eq!(run.workflow_version, 1);
        assert!(run.finished_at.is_some());
        assert!(run.total_duration_ms.is_some());
    }

    // ── Test 15: total_cost_usd None when no steps have cost ──────────────────

    #[tokio::test]
    async fn test_executor_no_cost_steps_total_cost_none() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow("cost_none", vec![shell_step("s1", "echo hi")]);

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert!(
            run.total_cost_usd.is_none(),
            "expected None total_cost when no steps have cost"
        );
    }

    // ── Test 16: EventEmittingLogSink integration ─────────────────────────────
    //
    // When `run_workflow` is called with an already-wrapped `EventEmittingLogSink`,
    // the executor's `set_current_step` call correctly labels chunk events with
    // the right step index and step id.  The chronology of events is also
    // verified: RunStarted → StepStarted(0) → StepOutput(0) → StepCompleted(0)
    // → StepStarted(1) → StepOutput(1) → StepCompleted(1) → RunCompleted.

    #[tokio::test]
    async fn test_executor_event_emitting_log_sink_integration() {
        use crate::daemon::events::WorkflowEvent;
        use crate::workflow::EventEmittingLogSink;
        use tokio::sync::broadcast;

        // 2-step workflow where each step prints one line.
        #[cfg(windows)]
        let (cmd1, cmd2) = ("echo step-one", "echo step-two");
        #[cfg(not(windows))]
        let (cmd1, cmd2) = ("echo step-one", "echo step-two");

        let workflow = make_workflow(
            "eel_integration",
            vec![shell_step("s1", cmd1), shell_step("s2", cmd2)],
        );

        let run_id = Uuid::now_v7();
        let (event_tx, mut event_rx) = broadcast::channel::<WorkflowEvent>(64);

        let inner_sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let wrapped_sink = Arc::new(EventEmittingLogSink::new(
            inner_sink,
            event_tx.clone(),
            run_id,
            workflow.id,
        )) as Arc<dyn LogSink>;

        let run = run_workflow(
            &workflow,
            run_id,
            empty_trigger(),
            wrapped_sink,
            Some(event_tx),
            None,
        )
        .await;

        assert_eq!(run.status, RunStatus::Completed);

        // Collect all events that are buffered.
        let mut events: Vec<WorkflowEvent> = Vec::new();
        loop {
            match event_rx.try_recv() {
                Ok(e) => events.push(e),
                Err(_) => break,
            }
        }

        // Must have at least RunStarted + 2×StepStarted + at least 1 StepOutput
        // per step + 2×StepCompleted + RunCompleted.
        assert!(
            !events.is_empty(),
            "No events were emitted; expected at least RunStarted"
        );

        // RunStarted must be first.
        assert!(
            matches!(events[0], WorkflowEvent::RunStarted { run_id: r, .. } if r == run_id),
            "First event should be RunStarted, got {:?}",
            events[0]
        );

        // RunCompleted must be last.
        let last = events.last().unwrap();
        assert!(
            matches!(last, WorkflowEvent::RunCompleted { .. })
                || matches!(last, WorkflowEvent::RunFailed { .. }),
            "Last event should be RunCompleted or RunFailed, got {:?}",
            last
        );

        // Every StepOutput event must have a non-empty step_id.
        let step_output_events: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let WorkflowEvent::StepOutput {
                    step_id,
                    step_index,
                    ..
                } = e
                {
                    Some((step_id.clone(), *step_index))
                } else {
                    None
                }
            })
            .collect();

        // We must have received at least one StepOutput per step.
        assert!(
            !step_output_events.is_empty(),
            "Expected at least one StepOutput event"
        );

        // All StepOutput events should have a step_index of 0 or 1 (executor
        // is 0-based: first step → index 0, second step → index 1).
        for (step_id, step_index) in &step_output_events {
            assert!(
                *step_index < 2,
                "step_index should be 0 or 1, got {} for step '{}'",
                step_index,
                step_id
            );
            assert!(
                step_id == "s1" || step_id == "s2",
                "Unknown step_id in StepOutput: '{}'",
                step_id
            );
        }
    }

    // ── Test 17: Kill signal aborts a long-running step ───────────────────────

    #[tokio::test]
    async fn test_kill_signal_aborts_long_running_step() {
        use std::collections::HashMap as StdHashMap;
        use tokio::sync::RwLock;

        // Use a platform-appropriate long-sleep command.
        #[cfg(windows)]
        let sleep_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
        #[cfg(not(windows))]
        let sleep_cmd = "sleep 30";

        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow("kill_test", vec![shell_step("long_step", sleep_cmd)]);
        let run_id = Uuid::now_v7();

        // Create a real kill_signals registry.
        let registry: Arc<RwLock<StdHashMap<Uuid, crate::workflow::step::KillSender>>> =
            Arc::new(RwLock::new(StdHashMap::new()));

        let registry_clone = Arc::clone(&registry);
        let run_id_clone = run_id;

        // Spawn a task that sends the kill signal after 200ms.
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if let Some(tx) = registry_clone.read().await.get(&run_id_clone) {
                let _ = tx.send(true);
            }
        });

        let start = std::time::Instant::now();
        let run = run_workflow(
            &workflow,
            run_id,
            empty_trigger(),
            sink,
            None,
            Some(registry),
        )
        .await;
        let elapsed = start.elapsed();

        assert_eq!(run.status, RunStatus::Killed, "run should be Killed");
        assert!(
            elapsed.as_secs() < 10,
            "run should finish well under 10s after kill (elapsed: {:?})",
            elapsed
        );
    }

    // ── Test 18: Kill signal only affects the target run ──────────────────────

    #[tokio::test]
    async fn test_kill_signal_only_affects_target_run() {
        use std::collections::HashMap as StdHashMap;
        use tokio::sync::RwLock;

        // Shared registry for both runs.
        let registry: Arc<RwLock<StdHashMap<Uuid, crate::workflow::step::KillSender>>> =
            Arc::new(RwLock::new(StdHashMap::new()));

        #[cfg(windows)]
        let sleep_cmd = "powershell -NoProfile -Command \"Start-Sleep -Seconds 30\"";
        #[cfg(not(windows))]
        let sleep_cmd = "sleep 30";

        // Run A: long-running — will be killed.
        let run_id_a = Uuid::now_v7();
        let sink_a = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow_a = make_workflow("kill_a", vec![shell_step("step_a", sleep_cmd)]);
        let registry_a = Arc::clone(&registry);

        // Run B: fast echo — should complete normally.
        let run_id_b = Uuid::now_v7();
        let sink_b = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow_b = make_workflow("echo_b", vec![shell_step("step_b", "echo hello")]);
        let registry_b = Arc::clone(&registry);

        // Kill-sender task for run A only.
        let registry_kill = Arc::clone(&registry);
        let kill_run_id = run_id_a;
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if let Some(tx) = registry_kill.read().await.get(&kill_run_id) {
                let _ = tx.send(true);
            }
        });

        // Run both concurrently.
        let (run_a, run_b) = tokio::join!(
            run_workflow(
                &workflow_a,
                run_id_a,
                empty_trigger(),
                sink_a,
                None,
                Some(registry_a),
            ),
            run_workflow(
                &workflow_b,
                run_id_b,
                empty_trigger(),
                sink_b,
                None,
                Some(registry_b),
            ),
        );

        assert_eq!(run_a.status, RunStatus::Killed, "run A should be Killed");
        assert_eq!(
            run_b.status,
            RunStatus::Completed,
            "run B should be Completed"
        );
    }

    // ── Test 20: step_index is non-zero and matches runtime order ────────────

    #[tokio::test]
    async fn test_step_run_step_index_matches_runtime_order() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "step_index_check",
            vec![
                shell_step("s1", "echo first"),
                shell_step("s2", "echo second"),
                shell_step("s3", "echo third"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.steps.len(), 3);

        // step_index should be 0, 1, 2 in order (0-based)
        assert_eq!(
            run.steps[0].step_index, 0,
            "first step should have step_index=0"
        );
        assert_eq!(
            run.steps[1].step_index, 1,
            "second step should have step_index=1"
        );
        assert_eq!(
            run.steps[2].step_index, 2,
            "third step should have step_index=2"
        );
    }

    // ── Test 21: Failed step records actual kind (not "unknown") ─────────────

    #[tokio::test]
    async fn test_failed_step_records_actual_kind() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "failed_kind_check",
            vec![shell_step_with_policy(
                "fail-shell",
                exit_one_cmd(),
                FailurePolicy::Abort,
            )],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Failed);
        assert_eq!(run.steps.len(), 1);
        // The failed shell step should record kind="shell", not "unknown"
        assert_eq!(
            run.steps[0].kind, "shell",
            "failed step should have kind='shell'"
        );
        assert_eq!(run.steps[0].step_index, 0, "step_index should be 0");
    }

    // ── Test 22: step_index is non-zero for non-abort failed steps ────────────

    #[tokio::test]
    async fn test_failed_continue_step_index_correct() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "failed_continue_idx",
            vec![
                shell_step("s1", "echo first"),
                shell_step_with_policy("s2-fail", exit_one_cmd(), FailurePolicy::Continue),
                shell_step("s3", "echo third"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.steps.len(), 3);

        let s1 = run.steps.iter().find(|r| r.step_id == "s1").unwrap();
        let s2 = run.steps.iter().find(|r| r.step_id == "s2-fail").unwrap();
        let s3 = run.steps.iter().find(|r| r.step_id == "s3").unwrap();

        assert_eq!(s1.step_index, 0);
        assert_eq!(s2.step_index, 1);
        assert_eq!(s3.step_index, 2);
        assert_eq!(
            s2.kind, "shell",
            "failed continue step should have kind='shell'"
        );
    }

    // ── Byte offsets: completed steps carry real start+end and are monotonic ─

    #[tokio::test]
    async fn test_completed_step_runs_carry_byte_offsets_and_monotonic() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow(
            "byte_offsets",
            vec![
                shell_step("s1", "echo first"),
                shell_step("s2", "echo second"),
            ],
        );

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        assert_eq!(run.steps.len(), 2);

        for step in &run.steps {
            assert!(
                step.log_byte_offset_end.is_some(),
                "completed step {} must have log_byte_offset_end populated",
                step.step_id
            );
            let end = step.log_byte_offset_end.unwrap();
            assert!(
                end > step.log_byte_offset_start,
                "step {} end ({}) must be > start ({})",
                step.step_id,
                end,
                step.log_byte_offset_start
            );
        }

        // Step 2's start must be at or past step 1's end (sink is append-only,
        // markers do not overlap).
        let prior_end = run.steps[0].log_byte_offset_end.unwrap();
        let next_start = run.steps[1].log_byte_offset_start;
        assert!(
            next_start >= prior_end,
            "subsequent step start ({}) must be >= prior step end ({})",
            next_start,
            prior_end
        );
    }

    // ── Test 19: Kill after run completion is a no-op ─────────────────────────

    #[tokio::test]
    async fn test_kill_after_run_completion_no_op() {
        use std::collections::HashMap as StdHashMap;
        use tokio::sync::RwLock;

        let registry: Arc<RwLock<StdHashMap<Uuid, crate::workflow::step::KillSender>>> =
            Arc::new(RwLock::new(StdHashMap::new()));

        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow("nop_kill", vec![shell_step("fast", "echo done")]);
        let run_id = Uuid::now_v7();

        // Run to completion.
        let run = run_workflow(
            &workflow,
            run_id,
            empty_trigger(),
            sink,
            None,
            Some(Arc::clone(&registry)),
        )
        .await;

        // The run should have completed normally.
        assert_eq!(run.status, RunStatus::Completed);

        // After the run finishes, the registry entry is removed.
        // Sending kill now should be a no-op (entry gone, no panic).
        let entry = registry.read().await.get(&run_id).cloned();
        assert!(
            entry.is_none(),
            "registry entry should be removed after run"
        );
    }

    // ── Tests for `TriggerParams.target_step` stdin routing ───────────────────

    /// Cross-platform "echo stdin to stdout" command.
    ///
    /// Unix: `cat` reads stdin and writes to stdout.
    /// Windows: PowerShell `[Console]::In.ReadToEnd()` mirrors stdin to stdout.
    fn cat_stdin_cmd() -> &'static str {
        #[cfg(windows)]
        {
            "powershell -NoProfile -Command \"[Console]::In.ReadToEnd()\""
        }
        #[cfg(not(windows))]
        {
            "cat"
        }
    }

    fn target_step_trigger(input: Value, target: &str) -> TriggerParams {
        TriggerParams {
            input,
            env: None,
            target_step: Some(target.to_string()),
        }
    }

    /// `target_step: "main"` + `input: "hello world"` routes the raw string
    /// bytes to the matching step's stdin.
    #[tokio::test]
    async fn test_target_step_routes_string_input_to_stdin() {
        let sink = Arc::new(MockLogSink::default());
        let workflow = make_workflow("target_str", vec![shell_step("main", cat_stdin_cmd())]);

        let trigger = target_step_trigger(json!("hello world"), "main");
        let run = run_workflow(
            &workflow,
            Uuid::now_v7(),
            trigger,
            Arc::clone(&sink) as Arc<dyn LogSink>,
            None,
            None,
        )
        .await;

        assert_eq!(run.status, RunStatus::Completed);
        let captured = String::from_utf8_lossy(&sink.chunks.lock().unwrap()).to_string();
        assert!(
            captured.contains("hello world"),
            "expected 'hello world' in step stdout (from stdin echo); got: {:?}",
            captured
        );
    }

    /// `target_step: "main"` + `input: {"foo":1}` routes compact-JSON bytes
    /// to the matching step's stdin.
    #[tokio::test]
    async fn test_target_step_routes_json_input_to_stdin() {
        let sink = Arc::new(MockLogSink::default());
        let workflow = make_workflow("target_json", vec![shell_step("main", cat_stdin_cmd())]);

        let trigger = target_step_trigger(json!({"foo": 1}), "main");
        let run = run_workflow(
            &workflow,
            Uuid::now_v7(),
            trigger,
            Arc::clone(&sink) as Arc<dyn LogSink>,
            None,
            None,
        )
        .await;

        assert_eq!(run.status, RunStatus::Completed);
        let captured = String::from_utf8_lossy(&sink.chunks.lock().unwrap()).to_string();
        assert!(
            captured.contains(r#"{"foo":1}"#),
            "expected compact JSON '{{\"foo\":1}}' in step stdout (from stdin echo); got: {:?}",
            captured
        );
    }

    /// `target_step: None` routes nothing automatically. A step with
    /// `pass_stdin: false` reads no stdin (process gets EOF immediately).
    #[tokio::test]
    async fn test_target_step_none_does_not_route_stdin() {
        let sink = Arc::new(MockLogSink::default());
        let workflow = make_workflow("no_target", vec![shell_step("main", cat_stdin_cmd())]);

        // Trigger has input but target_step is None → no stdin routing.
        let trigger = TriggerParams {
            input: json!("should not appear"),
            env: None,
            target_step: None,
        };
        let run = run_workflow(
            &workflow,
            Uuid::now_v7(),
            trigger,
            Arc::clone(&sink) as Arc<dyn LogSink>,
            None,
            None,
        )
        .await;

        assert_eq!(run.status, RunStatus::Completed);
        let captured = String::from_utf8_lossy(&sink.chunks.lock().unwrap()).to_string();
        assert!(
            !captured.contains("should not appear"),
            "input must NOT appear in stdout when target_step is None; got: {:?}",
            captured
        );
    }

    /// `target_step: "nonexistent"` is silently ignored. The workflow runs
    /// normally without stdin routing.
    #[tokio::test]
    async fn test_target_step_nonexistent_id_is_silently_ignored() {
        let sink = Arc::new(MockLogSink::default());
        let workflow = make_workflow("missing_target", vec![shell_step("main", cat_stdin_cmd())]);

        let trigger = target_step_trigger(json!("ignored input"), "does-not-exist");
        let run = run_workflow(
            &workflow,
            Uuid::now_v7(),
            trigger,
            Arc::clone(&sink) as Arc<dyn LogSink>,
            None,
            None,
        )
        .await;

        // Workflow completes normally — no error from the missing target_step.
        assert_eq!(run.status, RunStatus::Completed);
        let captured = String::from_utf8_lossy(&sink.chunks.lock().unwrap()).to_string();
        assert!(
            !captured.contains("ignored input"),
            "input must NOT appear when target_step references a missing step; got: {:?}",
            captured
        );
    }

    /// `target_step` pointing at a `MatchStep` is silently ignored — Match
    /// doesn't accept stdin. The workflow runs normally.
    #[tokio::test]
    async fn test_target_step_match_step_ignored() {
        let sink = Arc::new(MockLogSink::default());

        let mut cases = std::collections::HashMap::new();
        cases.insert(
            "A".to_string(),
            vec![shell_step("branch_a", "echo branch_a_ran")],
        );

        let workflow = make_workflow(
            "match_target",
            vec![StepDef::Match(MatchStep {
                common: StepDefCommon {
                    id: "match_step_id".to_string(),
                    on_failure: None,
                    always_run: false,
                    timeout_secs: None,
                    working_dir: None,
                    env_vars: None,
                    capture: CaptureSpec::default(),
                },
                expr: "A".to_string(),
                cases,
                default: None,
            })],
        );

        // target_step refers to the MatchStep — should be ignored silently.
        let trigger = target_step_trigger(json!("input bytes"), "match_step_id");
        let run = run_workflow(
            &workflow,
            Uuid::now_v7(),
            trigger,
            Arc::clone(&sink) as Arc<dyn LogSink>,
            None,
            None,
        )
        .await;

        assert_eq!(run.status, RunStatus::Completed);
        // The branch should still run.
        assert!(
            run.steps.iter().any(|r| r.step_id == "branch_a"),
            "branch_a should still execute"
        );
    }
}
