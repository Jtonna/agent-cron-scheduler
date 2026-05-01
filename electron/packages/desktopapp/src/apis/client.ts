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
  GlobalCostSummaryResponse,
  HealthResponse,
  Job,
  RecentRunsResponse,
  RunsResponse,
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
    return request<Job[]>("/api/jobs");
  },

  listRecentRuns(limit: number = 20): Promise<RecentRunsResponse> {
    return request<RecentRunsResponse>(`/api/runs/recent?limit=${limit}`);
  },

  getGlobalCostSummary(timeframe: string = "30d"): Promise<GlobalCostSummaryResponse> {
    return request<GlobalCostSummaryResponse>(
      `/api/costs/summary?timeframe=${encodeURIComponent(timeframe)}`,
    );
  },

  listJobRuns(jobId: string, limit: number = 20, offset: number = 0): Promise<RunsResponse> {
    return request<RunsResponse>(`/api/jobs/${jobId}/runs?limit=${limit}&offset=${offset}`);
  },
};
