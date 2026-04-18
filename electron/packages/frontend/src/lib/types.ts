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

export interface NewJob {
  name: string;
  schedule: string;
  execution: { type: "ShellCommand" | "ScriptFile"; value: string };
  enabled?: boolean;
  timezone?: string;
  working_dir?: string;
  env_vars?: Record<string, string>;
  timeout_secs?: number;
  log_environment?: boolean;
  allow_concurrent?: boolean;
  schedule_mode?: "Cron" | "WaitForCompletion";
  pre_hook?: string;
  post_hook?: string;
  pre_hook_script_type?: string;
  post_hook_script_type?: string;
}

export interface JobUpdate {
  name?: string;
  schedule?: string;
  execution?: { type: "ShellCommand" | "ScriptFile"; value: string };
  enabled?: boolean;
  timezone?: string | null;
  working_dir?: string | null;
  env_vars?: Record<string, string> | null;
  timeout_secs?: number;
  log_environment?: boolean;
  allow_concurrent?: boolean;
  schedule_mode?: "Cron" | "WaitForCompletion";
  pre_hook?: string | null;
  post_hook?: string | null;
  pre_hook_script_type?: string | null;
  post_hook_script_type?: string | null;
}

export interface JobRun {
  run_id: string;
  job_id: string;
  started_at: string;
  finished_at: string | null;
  status: "Running" | "Completed" | "CompletedWithWarnings" | "Failed" | "Killed";
  exit_code: number | null;
  log_size_bytes: number;
  error: string | null;
  trigger_params?: TriggerParams;
  total_cost_usd?: number | null;
  duration_ms?: number | null;
  num_turns?: number | null;
  model?: string | null;
  usage?: Record<string, unknown> | null;
}

export interface TriggerParams {
  args?: string;
  env?: Record<string, string>;
  input?: string;
}

export interface RunsResponse {
  runs: JobRun[];
  total: number;
  limit: number;
  offset: number;
}

export interface HealthResponse {
  status: string;
  uptime_seconds: number;
  active_jobs: number;
  total_jobs: number;
  version: string;
  data_dir: string;
}

export interface ServiceStatus {
  platform: string;
  service_installed: boolean;
  service_running: boolean;
}

export interface SavedConnection {
  id: string;
  label: string;
  url: string;
  addedAt: string;
}

export interface DailyDataPoint {
  date: string;
  runs: number;
  cost: number;
  input_tokens: number;
  output_tokens: number;
}

export interface CostSummary {
  total_runs: number;
  total_cost_usd: number;
  avg_cost_per_run: number;
  total_duration_ms: number;
  total_input_tokens: number;
  total_output_tokens: number;
  total_cache_read_tokens: number;
  runs_by_status: Record<string, number>;
}

export interface CostSummaryResponse {
  job_id: string;
  timeframe: string;
  summary: CostSummary;
  data: DailyDataPoint[];
}

export interface GlobalDailyTrend {
  date: string;
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
}

export interface GlobalTopJob {
  job_id: string;
  job_name: string;
  total_cost: number;
  total_runs: number;
}

export interface GlobalCostTokens {
  input: number;
  output: number;
}

export interface GlobalCostSummaryResponse {
  timeframe: string;
  today_usd: number;
  week_usd: number;
  month_usd: number;
  today_tokens: GlobalCostTokens;
  top_jobs: GlobalTopJob[];
  daily_trend: GlobalDailyTrend[];
}

export interface RecentRunEntry extends JobRun {
  job_name: string;
}

export interface RecentRunsResponse {
  runs: RecentRunEntry[];
  limit: number;
}
