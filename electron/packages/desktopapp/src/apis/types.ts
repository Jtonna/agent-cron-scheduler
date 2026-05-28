/* ── Workflow / Job (v4) ──
 *
 * The v4 backend replaced the "job" concept with "workflow + steps".
 * The desktopapp keeps the name `Job` for the workflow record to
 * minimize downstream churn — the wire shape now matches the
 * `GET /api/workflows` response.
 */

export interface CaptureConfig {
  stdout_max_bytes?: number | null;
  parser?: string | null;
  [key: string]: unknown;
}

interface WorkflowStepBase {
  id: string;
  always_run: boolean;
  on_failure: string;
  timeout_secs: number | null;
  working_dir: string | null;
  env_vars: Record<string, string> | null;
  capture: CaptureConfig;
}

export interface HttpWorkflowStep extends WorkflowStepBase {
  kind: "http";
  method: string;
  url: string;
  headers: Record<string, string> | null;
  body: string | null;
  expect_status: number[] | null;
}

export interface ShellWorkflowStep extends WorkflowStepBase {
  kind: "shell";
  command: string;
  pass_stdin: boolean;
}

export interface AgentWorkflowStep extends WorkflowStepBase {
  kind: "agent";
  agent_type: string;
  prompt: string;
  model: string | null;
  extra_args: string[] | null;
}

export type WorkflowStep = HttpWorkflowStep | ShellWorkflowStep | AgentWorkflowStep;

export interface Job {
  id: string;
  name: string;
  schedule: string;
  schedule_mode: "Cron" | string;
  enabled: boolean;
  allow_concurrent: boolean;
  on_failure: "abort" | string;
  steps: WorkflowStep[];
  timezone: string;
  working_dir: string;
  env_vars: Record<string, string> | null;
  default_input: Record<string, unknown> | null;
  created_at: string;
  updated_at: string;
  version: number;
  last_run_at: string | null;
  last_run_id: string | null;
  last_run_status: "Running" | "Completed" | "CompletedWithWarnings" | "Failed" | "Killed" | null;
  next_run_at: string | null;
}

export interface WorkflowSnapshot {
  id: string;
  name: string;
  version: number;
}

export interface WorkflowRunStep {
  step_index: number;
  step_id: string;
  kind: string;
  status: "Running" | "Completed" | "Failed" | "Killed";
  started_at: string;
  finished_at: string | null;
  exit_code: number | null;
  log_byte_offset_start: number;
  log_byte_offset_end: number;
  cost_usd: number | null;
  error: string | null;
}

export interface RecentRunEntry {
  run_id: string;
  workflow_id: string;
  workflow_version: number;
  workflow_snapshot: WorkflowSnapshot;
  started_at: string;
  finished_at: string | null;
  status: "Running" | "Completed" | "CompletedWithWarnings" | "Failed" | "Killed";
  trigger_input: Record<string, unknown> | null;
  steps: WorkflowRunStep[];
  total_cost_usd: number | null;
  total_duration_ms: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

export interface RecentRunsResponse {
  runs: RecentRunEntry[];
  total: number;
}

export interface HealthResponse {
  status: string;
  uptime_seconds: number;
  active_jobs: number;
  total_jobs: number;
  version: string;
  data_dir: string;
}

/* ── Dashboard cost data (GET /api/cost/workflows) ── */

export interface DailyCostBucket {
  date: string;
  runs_completed: number;
  runs_failed: number;
  runs_killed: number;
  cost_from_completed: number;
  cost_from_failed: number;
  cost_from_killed: number;
  total_usd: number;
  tokens_in_from_completed: number;
  tokens_in_from_failed: number;
  tokens_in_from_killed: number;
  tokens_out_from_completed: number;
  tokens_out_from_failed: number;
  tokens_out_from_killed: number;
  total_input_tokens: number;
  total_output_tokens: number;
}

export interface WorkflowCostSummary {
  computed_at: string;
  last_30_days_runs: number;
  last_30_days_total_usd: number;
  last_year_runs: number;
  last_year_total_usd: number;
  last_30_days_input_tokens: number;
  last_30_days_output_tokens: number;
  last_year_input_tokens: number;
  last_year_output_tokens: number;
  daily_buckets: DailyCostBucket[];
}

export interface WorkflowCostEntry {
  workflow_id: string;
  workflow_name: string;
  cost_summary: WorkflowCostSummary;
}

export interface CostWorkflowsResponse {
  system_cost_summary: WorkflowCostSummary;
  workflows: WorkflowCostEntry[];
}

/**
 * A run record. Per-workflow runs (`/api/workflows/{id}/runs`) and the
 * global recent-runs feed (`/api/runs/recent`) now return the exact same
 * shape, so `JobRun` is an alias of `RecentRunEntry`.
 */
export type JobRun = RecentRunEntry;

export interface RunsResponse {
  runs: JobRun[];
  total: number;
}

/* ── Workflow trigger (POST /api/workflows/{id}/trigger) ── */

/**
 * Per-invocation parameters for triggering a workflow. A body is required by
 * the backend's JSON extractor; send `{}` to fall back to
 * `workflow.default_input`. The three knobs are orthogonal:
 *   - `input`        — replaces `workflow.default_input` for this run
 *   - `env`          — overrides `workflow.env_vars` for this run (highest precedence)
 *   - `target_step`  — routes `input` to the named step's stdin
 */
export interface TriggerParams {
  input?: unknown;
  env?: Record<string, string>;
  target_step?: string;
}

export interface WorkflowTriggerResponse {
  run_id: string;
  workflow_id: string;
  workflow_version: number;
  run_url: string;
}
