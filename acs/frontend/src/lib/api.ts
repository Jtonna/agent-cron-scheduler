import type {
  Job,
  NewJob,
  JobUpdate,
  RunsResponse,
  HealthResponse,
  ServiceStatus,
} from "./types";

const BASE_URL = process.env.NEXT_PUBLIC_API_URL ?? "";

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

async function request<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = `${BASE_URL}${path}`;
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

async function requestText(
  path: string,
  options: RequestInit = {}
): Promise<string> {
  const url = `${BASE_URL}${path}`;
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

export const api = {
  health(): Promise<HealthResponse> {
    return request<HealthResponse>("/health");
  },

  listJobs(): Promise<Job[]> {
    return request<Job[]>("/api/jobs");
  },

  getJob(id: string): Promise<Job> {
    return request<Job>(`/api/jobs/${id}`);
  },

  createJob(job: NewJob): Promise<Job> {
    return request<Job>("/api/jobs", {
      method: "POST",
      body: JSON.stringify(job),
    });
  },

  updateJob(id: string, update: JobUpdate): Promise<Job> {
    return request<Job>(`/api/jobs/${id}`, {
      method: "PATCH",
      body: JSON.stringify(update),
    });
  },

  deleteJob(id: string): Promise<void> {
    return request<void>(`/api/jobs/${id}`, {
      method: "DELETE",
    });
  },

  enableJob(id: string): Promise<Job> {
    return request<Job>(`/api/jobs/${id}/enable`, {
      method: "POST",
    });
  },

  disableJob(id: string): Promise<Job> {
    return request<Job>(`/api/jobs/${id}/disable`, {
      method: "POST",
    });
  },

  triggerJob(id: string): Promise<{ run_id: string }> {
    return request<{ run_id: string }>(`/api/jobs/${id}/trigger`, {
      method: "POST",
    });
  },

  listRuns(
    jobId: string,
    limit: number = 20,
    offset: number = 0
  ): Promise<RunsResponse> {
    return request<RunsResponse>(
      `/api/jobs/${jobId}/runs?limit=${limit}&offset=${offset}`
    );
  },

  getRunLog(runId: string, tail?: number): Promise<string> {
    const params = tail ? `?tail=${tail}` : "";
    return requestText(`/api/runs/${runId}/log${params}`);
  },

  getSystemLogs(tail?: number): Promise<string> {
    const params = tail ? `?tail=${tail}` : "";
    return requestText(`/api/logs${params}`);
  },

  shutdown(): Promise<{ message: string }> {
    return request<{ message: string }>("/api/shutdown", {
      method: "POST",
    });
  },

  restart(): Promise<{ message: string }> {
    return request<{ message: string }>("/api/restart", {
      method: "POST",
    });
  },

  serviceStatus(): Promise<ServiceStatus> {
    return request<ServiceStatus>("/api/service/status");
  },
};
