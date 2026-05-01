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

export interface RecentRunEntry {
  run_id: string;
  job_id: string;
  job_name: string;
  started_at: string;
  finished_at: string | null;
  status: "Running" | "Completed" | "CompletedWithWarnings" | "Failed" | "Killed";
  exit_code: number | null;
  log_size_bytes: number;
  error: string | null;
  total_cost_usd?: number | null;
  duration_ms?: number | null;
  num_turns?: number | null;
  model?: string | null;
  usage?: Record<string, unknown> | null;
}

export interface RecentRunsResponse {
  runs: RecentRunEntry[];
  limit: number;
}

export interface HealthResponse {
  status: string;
  uptime_seconds: number;
  active_jobs: number;
  total_jobs: number;
  version: string;
  data_dir: string;
}

export interface GlobalCostTokens {
  input: number;
  output: number;
}

export interface GlobalTopJob {
  job_id: string;
  job_name: string;
  total_cost: number;
  total_runs: number;
}

export interface GlobalDailyTrend {
  date: string;
  cost_usd: number;
  input_tokens: number;
  output_tokens: number;
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
