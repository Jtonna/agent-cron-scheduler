export interface Job {
  id: string;
  name: string;
  schedule: string;
  execution: { type: "ShellCommand" | "ScriptFile"; value: string };
  enabled: boolean;
  timezone: string | null;
  working_dir: string | null;
  env_vars: Record<string, string> | null;
  timeout_secs: number;
  log_environment: boolean;
  allow_concurrent: boolean;
  schedule_mode?: "Cron" | "WaitForCompletion";
  pre_hook: string | null;
  post_hook: string | null;
  pre_hook_script_type: string | null;
  post_hook_script_type: string | null;
  created_at: string;
  updated_at: string;
  last_run_at: string | null;
  last_exit_code: number | null;
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

export interface JobRun {
  run_id: string;
  job_id: string;
  started_at: string;
  finished_at: string | null;
  status: "Running" | "Completed" | "CompletedWithWarnings" | "Failed" | "Killed";
  exit_code: number | null;
  total_cost_usd?: number | null;
  duration_ms?: number | null;
}

export interface RunsResponse {
  runs: JobRun[];
  total: number;
  limit: number;
  offset: number;
}

/* ── Per-job cost summary (GET /api/jobs/{id}/cost-summary) ── */

export interface JobDailyCostDataPoint {
  date: string;
  runs: number;
  cost: number;
  input_tokens: number;
  output_tokens: number;
}

export interface JobCostSummary {
  total_runs: number;
  total_cost_usd: number;
  avg_cost_per_run: number;
  total_duration_ms: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  runs_by_status: Record<string, number>;
}

export interface JobCostSummaryResponse {
  job_id: string;
  timeframe: string;
  summary: JobCostSummary;
  data: JobDailyCostDataPoint[];
}
