import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    listJobs: vi.fn(),
  },
}));

import { api } from "@/lib/api";
import { useJobs } from "../useJobs";

const mockApi = api as { listJobs: ReturnType<typeof vi.fn> };

const mockJobs = [
  {
    id: "job-1",
    name: "Daily Backup",
    schedule: "0 0 * * *",
    execution: { type: "ShellCommand" as const, value: "backup.sh" },
    enabled: true,
    timezone: null,
    working_dir: null,
    env_vars: null,
    timeout_secs: 3600,
    log_environment: false,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: "2024-01-01T00:00:00Z",
    updated_at: "2024-01-01T00:00:00Z",
    last_run_at: null,
    last_exit_code: null,
    next_run_at: "2024-01-02T00:00:00Z",
  },
  {
    id: "job-2",
    name: "Weekly Report",
    schedule: "0 9 * * 1",
    execution: { type: "ShellCommand" as const, value: "report.sh" },
    enabled: true,
    timezone: "America/New_York",
    working_dir: "/app",
    env_vars: { NODE_ENV: "production" },
    timeout_secs: 1800,
    log_environment: true,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: "2024-01-01T00:00:00Z",
    updated_at: "2024-01-01T00:00:00Z",
    last_run_at: "2024-01-08T09:00:00Z",
    last_exit_code: 0,
    next_run_at: "2024-01-15T09:00:00Z",
  },
];

describe("useJobs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with loading true", () => {
    mockApi.listJobs.mockResolvedValue(mockJobs);
    const { result } = renderHook(() => useJobs());
    expect(result.current.loading).toBe(true);
  });

  it("returns jobs array on successful fetch", async () => {
    mockApi.listJobs.mockResolvedValue(mockJobs);
    const { result } = renderHook(() => useJobs());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.jobs).toEqual(mockJobs);
    expect(result.current.error).toBeNull();
  });

  it("starts with empty jobs array", () => {
    mockApi.listJobs.mockResolvedValue(mockJobs);
    const { result } = renderHook(() => useJobs());
    expect(result.current.jobs).toEqual([]);
  });

  it("sets error on API failure", async () => {
    mockApi.listJobs.mockRejectedValue(new Error("Failed to fetch jobs"));
    const { result } = renderHook(() => useJobs());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe("Failed to fetch jobs");
    expect(result.current.jobs).toEqual([]);
  });

  it("handles non-array response gracefully", async () => {
    mockApi.listJobs.mockResolvedValue(null);
    const { result } = renderHook(() => useJobs());

    await waitFor(() => expect(result.current.loading).toBe(false));

    // Non-array result means jobs remains unchanged (empty)
    expect(result.current.jobs).toEqual([]);
    expect(result.current.error).toBeNull();
  });

  it("refresh triggers re-fetch", async () => {
    mockApi.listJobs.mockResolvedValue(mockJobs);
    const { result } = renderHook(() => useJobs());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(mockApi.listJobs).toHaveBeenCalledTimes(1);

    const newJob = {
      id: "job-3",
      name: "New Job",
      schedule: "*/5 * * * *",
      execution: { type: "ShellCommand" as const, value: "new-job.sh" },
      enabled: false,
      timezone: null,
      working_dir: null,
      env_vars: null,
      timeout_secs: 300,
      log_environment: false,
      allow_concurrent: true,
      pre_hook: null,
      post_hook: null,
      pre_hook_script_type: null,
      post_hook_script_type: null,
      created_at: "2024-01-02T00:00:00Z",
      updated_at: "2024-01-02T00:00:00Z",
      last_run_at: null,
      last_exit_code: null,
      next_run_at: null,
    };
    mockApi.listJobs.mockResolvedValue([...mockJobs, newJob]);

    await act(async () => {
      await result.current.refresh();
    });

    expect(mockApi.listJobs).toHaveBeenCalledTimes(2);
    expect(result.current.jobs).toHaveLength(3);
    expect(result.current.jobs[2]).toEqual(newJob);
  });
});
