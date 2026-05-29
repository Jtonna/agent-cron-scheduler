interface AcsWindow {
  __ACS_API_URL__?: string;
}

export function getBaseUrl(): string {
  if (typeof window !== "undefined") {
    const w = window as Window & AcsWindow;
    return w.__ACS_API_URL__ ?? process.env.NEXT_PUBLIC_API_URL ?? "http://127.0.0.1:8377";
  }
  return "http://127.0.0.1:8377";
}

export class ApiError extends Error {
  status: number;
  code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

async function request<T>(path: string, options: RequestInit = {}): Promise<T> {
  const url = `${getBaseUrl()}${path}`;
  const res = await fetch(url, {
    ...options,
    headers: {
      "Content-Type": "application/json",
      ...options.headers,
    },
  });

  if (!res.ok) {
    let code = "UNKNOWN";
    let message = `Request failed with status ${res.status}`;
    try {
      const body = await res.json();
      code = body.code || code;
      message = body.message || body.error || message;
    } catch {
      // ignore parse errors
    }
    throw new ApiError(res.status, code, message);
  }

  const contentType = res.headers.get("content-type");
  if (contentType && contentType.includes("application/json")) {
    return res.json();
  }
  return undefined as unknown as T;
}

async function requestText(path: string, options: RequestInit = {}): Promise<string> {
  const url = `${getBaseUrl()}${path}`;
  const res = await fetch(url, options);

  if (!res.ok) {
    let code = "UNKNOWN";
    let message = `Request failed with status ${res.status}`;
    try {
      const body = await res.json();
      code = body.code || code;
      message = body.message || body.error || message;
    } catch {
      // ignore parse errors
    }
    throw new ApiError(res.status, code, message);
  }

  return res.text();
}

import type {
  CostWorkflowsResponse,
  HealthResponse,
  Job,
  JobRun,
  RecentRunsResponse,
  RunsResponse,
  TriggerParams,
  WorkflowCostEntry,
  WorkflowTriggerResponse,
} from "./types";

export const api = {
  getSystemLogs(tail?: number): Promise<string> {
    const params = tail ? `?tail=${tail}` : "";
    return requestText(`/api/logs${params}`);
  },

  health(): Promise<HealthResponse> {
    return request<HealthResponse>("/health");
  },

  listJobs(): Promise<Job[]> {
    return request<Job[]>("/api/workflows");
  },

  getJob(id: string): Promise<Job> {
    return request<Job>(`/api/workflows/${id}`);
  },

  getWorkflowCost(workflowId: string): Promise<WorkflowCostEntry> {
    return request<WorkflowCostEntry>(`/api/cost/workflows/${workflowId}`);
  },

  listRecentRuns(limit: number = 20, offset: number = 0): Promise<RecentRunsResponse> {
    return request<RecentRunsResponse>(`/api/runs/recent?limit=${limit}&offset=${offset}`);
  },

  getCostWorkflows(): Promise<CostWorkflowsResponse> {
    return request<CostWorkflowsResponse>("/api/cost/workflows");
  },

  listJobRuns(jobId: string, limit: number = 20, offset: number = 0): Promise<RunsResponse> {
    return request<RunsResponse>(`/api/workflows/${jobId}/runs?limit=${limit}&offset=${offset}`);
  },

  getRun(runId: string): Promise<JobRun> {
    return request<JobRun>(`/api/runs/${runId}`);
  },

  getRunLog(runId: string, stepIndex?: number): Promise<string> {
    const params = stepIndex !== undefined ? `?step_index=${stepIndex}` : "";
    return requestText(`/api/runs/${runId}/log${params}`);
  },

  killRun(runId: string): Promise<void> {
    return request<void>(`/api/runs/${runId}/kill`, { method: "POST" }) as Promise<void>;
  },

  updateWorkflow(id: string, update: WorkflowUpdate): Promise<Job> {
    return request<Job>(`/api/workflows/${id}`, {
      method: "PATCH",
      body: JSON.stringify(update),
    });
  },

  toggleWorkflowEnabled(id: string, newEnabled: boolean): Promise<Job> {
    return request<Job>(`/api/workflows/${id}`, {
      method: "PATCH",
      body: JSON.stringify({ enabled: newEnabled }),
    });
  },

  triggerWorkflow(id: string, params: TriggerParams = {}): Promise<WorkflowTriggerResponse> {
    return request<WorkflowTriggerResponse>(`/api/workflows/${id}/trigger`, {
      method: "POST",
      body: JSON.stringify(params),
    });
  },

  favoriteWorkflow(id: string): Promise<Job> {
    return request<Job>(`/api/workflows/${id}/favorite`, { method: "POST" });
  },

  unfavoriteWorkflow(id: string): Promise<Job> {
    return request<Job>(`/api/workflows/${id}/favorite`, { method: "DELETE" });
  },

  /**
   * Create a new workflow. The body shape must match the backend
   * `POST /api/workflows` request — see `acs/src/models/workflow.rs`.
   * Validation errors come back as standard ApiError with the
   * server-provided code/message.
   */
  createWorkflow(body: Record<string, unknown>): Promise<Job> {
    return request<Job>("/api/workflows", {
      method: "POST",
      body: JSON.stringify(body),
    });
  },
};

export type WorkflowUpdate = Record<string, unknown>;
