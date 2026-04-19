import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useJobs } from "../useJobs";
import * as api from "@/lib/api";

vi.mock("@/lib/api");

describe("useJobs", () => {
  const mockJobs = [
    {
      id: "job-1",
      name: "Job 1",
      schedule: "0 0 * * *",
      execution: { type: "ShellCommand" as const, value: "echo test" },
      enabled: true,
      timezone: "UTC",
      working_dir: null,
      env_vars: null,
      timeout_secs: 300,
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
      name: "Job 2",
      schedule: "0 12 * * *",
      execution: { type: "ShellCommand" as const, value: "echo test2" },
      enabled: false,
      timezone: null,
      working_dir: null,
      env_vars: null,
      timeout_secs: 300,
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
      next_run_at: null,
    },
  ];

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it("fetches jobs on mount", async () => {
    (api.api.listJobs as any).mockResolvedValue(mockJobs);

    const { result } = renderHook(() => useJobs());

    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.jobs).toEqual(mockJobs);
    expect(result.current.error).toBeNull();
    expect(api.api.listJobs).toHaveBeenCalled();
  });

  it("handles fetch error", async () => {
    const errorMessage = "Failed to fetch";
    (api.api.listJobs as any).mockRejectedValue(
      new Error(errorMessage)
    );

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe(errorMessage);
    expect(result.current.jobs).toEqual([]);
  });

  it("refresh function fetches fresh jobs", async () => {
    (api.api.listJobs as any).mockResolvedValue(mockJobs);

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.jobs).toEqual(mockJobs);

    const newJob = {
      ...mockJobs[0],
      id: "job-3",
      name: "Job 3",
    };
    (api.api.listJobs as any).mockClear();
    (api.api.listJobs as any).mockResolvedValue([...mockJobs, newJob]);

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.jobs).toHaveLength(3);
  });

  it("triggerJob calls api and refreshes", async () => {
    (api.api.listJobs as any).mockResolvedValue(mockJobs);
    (api.api.triggerJob as any).mockResolvedValue({
      message: "Job triggered",
      job_id: "job-1",
      job_name: "Job 1",
      run_id: "run-1",
    });

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const callCountBefore = vi.mocked(api.api.listJobs).mock.calls.length;

    await act(async () => {
      await result.current.triggerJob("job-1");
    });

    expect(api.api.triggerJob).toHaveBeenCalledWith("job-1");
    expect(vi.mocked(api.api.listJobs).mock.calls.length).toBe(callCountBefore + 1);
  });

  it("enableJob calls api and refreshes", async () => {
    (api.api.listJobs as any).mockResolvedValue(mockJobs);
    (api.api.enableJob as any).mockResolvedValue({
      ...mockJobs[1],
      enabled: true,
    });

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const callCountBefore = vi.mocked(api.api.listJobs).mock.calls.length;

    await act(async () => {
      await result.current.enableJob("job-2");
    });

    expect(api.api.enableJob).toHaveBeenCalledWith("job-2");
    expect(vi.mocked(api.api.listJobs).mock.calls.length).toBe(callCountBefore + 1);
  });

  it("disableJob calls api and refreshes", async () => {
    (api.api.listJobs as any).mockResolvedValue(mockJobs);
    (api.api.disableJob as any).mockResolvedValue({
      ...mockJobs[0],
      enabled: false,
    });

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const callCountBefore = vi.mocked(api.api.listJobs).mock.calls.length;

    await act(async () => {
      await result.current.disableJob("job-1");
    });

    expect(api.api.disableJob).toHaveBeenCalledWith("job-1");
    expect(vi.mocked(api.api.listJobs).mock.calls.length).toBe(callCountBefore + 1);
  });

  it("deleteJob calls api and refreshes", async () => {
    (api.api.listJobs as any).mockResolvedValue(mockJobs);
    (api.api.deleteJob as any).mockResolvedValue(undefined);

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const callCountBefore = vi.mocked(api.api.listJobs).mock.calls.length;

    await act(async () => {
      await result.current.deleteJob("job-1");
    });

    expect(api.api.deleteJob).toHaveBeenCalledWith("job-1");
    expect(vi.mocked(api.api.listJobs).mock.calls.length).toBe(callCountBefore + 1);
  });

  it("clears error on successful fetch", async () => {
    (api.api.listJobs as any).mockRejectedValueOnce(
      new Error("Network error")
    );

    const { result } = renderHook(() => useJobs());

    await waitFor(() => {
      expect(result.current.error).toBe("Network error");
    });

    (api.api.listJobs as any).mockClear();
    (api.api.listJobs as any).mockResolvedValue(mockJobs);

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.error).toBeNull();
    expect(result.current.jobs).toEqual(mockJobs);
  });
});
