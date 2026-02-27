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
}

export interface JobRun {
  run_id: string;
  job_id: string;
  started_at: string;
  finished_at: string | null;
  status: "Running" | "Completed" | "Failed" | "Killed";
  exit_code: number | null;
  log_size_bytes: number;
  error: string | null;
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
