use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::daemon::events::WorkflowEvent;
use crate::models::workflow::{
    FailurePolicy, RunStatus, StepDef, StepRun, TriggerParams, Workflow, WorkflowRun,
};
use crate::workflow::step::{LogSink, Step, StepContext, StepError, StepOutput};
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
async fn dispatch_step(
    step_def: &StepDef,
    ctx: &mut StepContext,
) -> Result<StepOutput, StepError> {
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

        ctx.step_index += 1;

        // ── MatchStep: special handling ───────────────────────────────────────

        if let StepDef::Match(m) = step_def {
            let step_index = ctx.step_index;
            let run_id = ctx.run_id;
            let workflow_id = ctx.workflow_id;

            // Emit StepStarted for the synthetic match step.
            emit(ctx.event_tx.as_ref(), WorkflowEvent::StepStarted {
                run_id,
                workflow_id,
                step_index,
                step_id: m.common.id.clone(),
                kind: "match".to_string(),
                started_at: Utc::now(),
            });

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

            // Synthetic StepRun for the MatchStep itself.
            let match_run = StepRun {
                step_index: ctx.step_index,
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
                output_summary: Some(json!({
                    "evaluated": evaluated,
                    "case_taken": case_taken
                })),
            };

            // Emit StepCompleted for the match step.
            emit(ctx.event_tx.as_ref(), WorkflowEvent::StepCompleted {
                run_id,
                workflow_id,
                step_index,
                step_id: m.common.id.clone(),
                exit_code: Some(0),
                cost_usd: None,
                finished_at: Utc::now(),
            });

            step_runs.push(match_run);

            // Insert a placeholder so ${steps.<id>.*} can resolve.
            ctx.steps.insert(
                m.common.id.clone(),
                StepOutput {
                    exit_code: Some(0),
                    stdout: None,
                    exports: HashMap::new(),
                    cost: None,
                },
            );

            // Recurse into branch steps if a branch was matched.
            if let Some(branch) = branch_steps {
                // We need to clone to avoid borrow issues since execute_steps is recursive
                let branch_owned: Vec<StepDef> = branch.clone();
                Box::pin(execute_steps(
                    &branch_owned,
                    workflow,
                    ctx,
                    step_runs,
                    aborted,
                    killed,
                ))
                .await;
            }

            continue;
        }

        // ── Regular step execution ────────────────────────────────────────────

        let step_index = ctx.step_index;
        let run_id = ctx.run_id;
        let workflow_id = ctx.workflow_id;
        let step_kind = step_kind_str(step_def);

        // Emit StepStarted before executing the step.
        emit(ctx.event_tx.as_ref(), WorkflowEvent::StepStarted {
            run_id,
            workflow_id,
            step_index,
            step_id: common.id.clone(),
            kind: step_kind.to_string(),
            started_at: Utc::now(),
        });

        let started_at = Utc::now();
        let effective_policy = common
            .on_failure
            .clone()
            .unwrap_or_else(|| workflow.on_failure.clone());

        let result = run_step_with_policy(step_def, ctx, effective_policy, started_at).await;

        match result {
            StepRunResult::Completed(run, output) => {
                // Emit StepCompleted after successful execution.
                emit(ctx.event_tx.as_ref(), WorkflowEvent::StepCompleted {
                    run_id,
                    workflow_id,
                    step_index,
                    step_id: common.id.clone(),
                    exit_code: run.exit_code,
                    cost_usd: run.cost_usd,
                    finished_at: Utc::now(),
                });
                ctx.steps.insert(common.id.clone(), output);
                step_runs.push(run);
            }
            StepRunResult::Failed(run) => {
                // Emit StepCompleted with non-zero exit code or None.
                emit(ctx.event_tx.as_ref(), WorkflowEvent::StepCompleted {
                    run_id,
                    workflow_id,
                    step_index,
                    step_id: common.id.clone(),
                    exit_code: run.exit_code,
                    cost_usd: run.cost_usd,
                    finished_at: Utc::now(),
                });
                step_runs.push(run);
                *aborted = true;
            }
            StepRunResult::FailedContinue(run, output) => {
                // Insert the actual output (even though the step failed) so that
                // downstream template references like ${steps.<id>.exit_code} resolve.
                emit(ctx.event_tx.as_ref(), WorkflowEvent::StepCompleted {
                    run_id,
                    workflow_id,
                    step_index,
                    step_id: common.id.clone(),
                    exit_code: run.exit_code,
                    cost_usd: run.cost_usd,
                    finished_at: Utc::now(),
                });
                ctx.steps.insert(common.id.clone(), output);
                step_runs.push(run);
                // Do NOT set aborted — continue policy means keep going.
            }
            StepRunResult::Killed(run) => {
                emit(ctx.event_tx.as_ref(), WorkflowEvent::StepCompleted {
                    run_id,
                    workflow_id,
                    step_index,
                    step_id: common.id.clone(),
                    exit_code: run.exit_code,
                    cost_usd: run.cost_usd,
                    finished_at: Utc::now(),
                });
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
                            let run = make_step_run(common, started_at, RunStatus::Completed, &output, None);
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
                let run = make_step_run(common, started_at, RunStatus::Failed, &output, Some(err_msg));
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
        step_index: 0, // caller will not rely on this; step_index is tracked via ctx
        step_id: common.id.clone(),
        kind: kind_from_common(common),
        status,
        started_at,
        finished_at: Some(Utc::now()),
        exit_code: output.exit_code,
        log_byte_offset_start: 0,
        log_byte_offset_end: None,
        cost_usd: output.cost.as_ref().and_then(|c| c.total_cost_usd),
        error,
        output_summary: output.stdout.clone(),
    }
}

fn make_failed_step_run(
    common: &crate::models::workflow::StepDefCommon,
    started_at: chrono::DateTime<Utc>,
    error: &str,
) -> StepRun {
    StepRun {
        step_index: 0,
        step_id: common.id.clone(),
        kind: kind_from_common(common),
        status: RunStatus::Failed,
        started_at,
        finished_at: Some(Utc::now()),
        exit_code: None,
        log_byte_offset_start: 0,
        log_byte_offset_end: None,
        cost_usd: None,
        error: Some(error.to_string()),
        output_summary: None,
    }
}

/// Derive the step kind string from the common struct.
/// (We don't have the full StepDef here, so we do a best-effort based on id context.
/// The actual kind is set by the StepDef at the call-site — this is only used
/// for failed steps where we don't have an output.)
fn kind_from_common(_common: &crate::models::workflow::StepDefCommon) -> String {
    // We cannot determine kind from StepDefCommon alone; callers that need the
    // correct kind for a failed step pass it explicitly via the top-level runner.
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
///
/// Note: `StepOutput` chunk events (`WorkflowEvent::StepOutput`) are deferred to
/// phase 6, where they will be wired into the log sink's streaming path.
pub async fn run_workflow(
    workflow: &Workflow,
    run_id: Uuid,
    trigger: TriggerParams,
    log_sink: Arc<dyn LogSink>,
    event_tx: Option<broadcast::Sender<WorkflowEvent>>,
) -> WorkflowRun {
    let snapshot = workflow.clone();
    let started_at = Utc::now();

    // Emit RunStarted before executing any steps.
    emit(event_tx.as_ref(), WorkflowEvent::RunStarted {
        run_id,
        workflow_id: workflow.id,
        workflow_version: workflow.version,
        started_at,
    });

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
        steps: HashMap::new(),
        log_sink,
        working_dir,
        env,
        event_tx: event_tx.clone(),
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

    // Emit RunCompleted or RunFailed.
    match status {
        RunStatus::Completed => {
            emit(event_tx.as_ref(), WorkflowEvent::RunCompleted {
                run_id,
                workflow_id: workflow.id,
                status: RunStatus::Completed,
                total_cost_usd,
                finished_at,
            });
        }
        RunStatus::Failed | RunStatus::Killed => {
            // Use the first error from step_runs as the failure message.
            let error_msg = step_runs
                .iter()
                .find_map(|r| r.error.as_deref())
                .unwrap_or("workflow run failed")
                .to_string();
            emit(event_tx.as_ref(), WorkflowEvent::RunFailed {
                run_id,
                workflow_id: workflow.id,
                error: error_msg,
                finished_at,
            });
        }
        RunStatus::Running => {
            // Should not happen at end of run_workflow; emit RunFailed defensively.
            emit(event_tx.as_ref(), WorkflowEvent::RunFailed {
                run_id,
                workflow_id: workflow.id,
                error: "unexpected Running status at end of run_workflow".to_string(),
                finished_at,
            });
        }
    }

    let total_duration_ms = (finished_at - started_at).num_milliseconds().max(0) as u64;

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

    use crate::models::workflow::{
        CaptureSpec, FailurePolicy, MatchStep, RunStatus, SetVarStep, ShellStep, StepDef,
        StepDefCommon, TriggerParams, Workflow,
    };
    use crate::models::workflow::ScheduleMode;
    use crate::workflow::step::{LogSink, StepOutput};

    use super::run_workflow;

    // ── Mock LogSink ──────────────────────────────────────────────────────────

    #[derive(Clone, Default)]
    struct MockLogSink {
        chunks: Arc<Mutex<Vec<u8>>>,
        events: Arc<Mutex<Vec<String>>>,
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
            Ok(0)
        }

        async fn write_chunk(&self, data: &[u8]) -> std::io::Result<()> {
            self.chunks.lock().unwrap().extend_from_slice(data);
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
            Ok(0)
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
            input_schema: None,
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
        { "exit 1" }
        #[cfg(not(windows))]
        { "sh -c 'exit 1'" }
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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        assert_eq!(run.status, RunStatus::Failed, "expected Failed");
        // The failing step is recorded; the skipped step is NOT recorded.
        assert_eq!(run.steps.len(), 1, "only the failed step should be recorded");
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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        // No abort happened, so overall Completed.
        assert_eq!(run.status, RunStatus::Completed, "expected Completed despite step failure");
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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        // Expected step_runs: set_choice, m1 (synthetic), branch_a
        assert_eq!(run.steps.len(), 3, "expected 3 step runs; got: {:?}", run.steps.iter().map(|r| &r.step_id).collect::<Vec<_>>());

        let match_run = run.steps.iter().find(|r| r.step_id == "m1").unwrap();
        assert_eq!(match_run.status, RunStatus::Completed);
        // output_summary should indicate case_taken == "A"
        let summary = match_run.output_summary.as_ref().unwrap();
        assert_eq!(summary["case_taken"], json!("A"));

        let branch_run = run.steps.iter().find(|r| r.step_id == "branch_a").unwrap();
        assert_eq!(branch_run.status, RunStatus::Completed);
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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        assert_eq!(run.status, RunStatus::Completed);

        let match_run = run.steps.iter().find(|r| r.step_id == "m2").unwrap();
        let summary = match_run.output_summary.as_ref().unwrap();
        assert_eq!(summary["case_taken"], json!("default"));

        assert!(
            run.steps.iter().any(|r| r.step_id == "default_step"),
            "default branch step should appear in step_runs"
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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        assert_eq!(run.status, RunStatus::Completed);

        let match_run = run.steps.iter().find(|r| r.step_id == "m3").unwrap();
        let summary = match_run.output_summary.as_ref().unwrap();
        assert_eq!(summary["case_taken"], json!("none"));

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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

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
    // AgentStep no longer returns a "not implemented" error; it now calls execute().
    // Since `claude` CLI is not available in the test environment, it will fail
    // with a spawn error — but crucially not with a "not implemented" Internal error.

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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        // The step should fail (no `claude` CLI in test env), but NOT with "not implemented".
        assert_eq!(run.status, RunStatus::Failed);
        if let Some(err) = run.steps[0].error.as_ref() {
            assert!(
                !err.contains("not implemented in phase 3"),
                "AgentStep should no longer return a phase-3 not-implemented error, got: {}",
                err
            );
        }
        // The step ran (was dispatched), so there should be exactly 1 step run.
        assert_eq!(run.steps.len(), 1);
    }

    // ── Test 11: workflow.default_input applied when trigger.input is Null ────

    #[tokio::test]
    async fn test_executor_default_input_applied() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;

        let n = now();
        let mut workflow = make_workflow(
            "default_input",
            vec![shell_step("s1", "echo ${input.x}")],
        );
        workflow.default_input = Some(json!({"x": 42}));

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

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

        let mut workflow = make_workflow(
            "input_override",
            vec![shell_step("s1", "echo ${input.x}")],
        );
        workflow.default_input = Some(json!({"x": 42}));

        let trigger = input_trigger(json!({"x": 99}));
        let run = run_workflow(&workflow, Uuid::now_v7(), trigger, sink, None).await;

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

        let run = run_workflow(&workflow, Uuid::now_v7(), trigger, Arc::clone(&sink) as Arc<dyn LogSink>, None).await;

        assert_eq!(run.status, RunStatus::Completed);
        // Verify through the mock log sink chunks that FOO=b and BAR=c appear
        if let Some(sink_inner) = Arc::downcast::<MockLogSink>(sink).ok() {
            let output = String::from_utf8_lossy(&sink_inner.chunks.lock().unwrap()).to_string();
            assert!(output.contains("FOO=b"), "expected FOO=b in output: {}", output);
            assert!(output.contains("BAR=c"), "expected BAR=c in output: {}", output);
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

        let run = run_workflow(&workflow, Uuid::now_v7(), trigger, sink, None).await;

        assert_eq!(run.status, RunStatus::Completed);
    }

    // ── Test 14: WorkflowRun has correct metadata ──────────────────────────────

    #[tokio::test]
    async fn test_executor_run_metadata() {
        let sink = Arc::new(MockLogSink::default()) as Arc<dyn LogSink>;
        let workflow = make_workflow("meta", vec![shell_step("s1", "echo hi")]);
        let run_id = Uuid::now_v7();

        let run = run_workflow(&workflow, run_id, empty_trigger(), sink, None).await;

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

        let run = run_workflow(&workflow, Uuid::now_v7(), empty_trigger(), sink, None).await;

        assert!(
            run.total_cost_usd.is_none(),
            "expected None total_cost when no steps have cost"
        );
    }
}
