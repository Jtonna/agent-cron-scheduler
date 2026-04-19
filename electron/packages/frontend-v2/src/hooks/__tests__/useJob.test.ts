import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { useJob } from "../useJob";
import * as api from "@/lib/api";

vi.mock("@/lib/api");

describe("useJob", () => {
  const mockJob = {
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
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.resetAllMocks();
  });

  it("fetches job on mount with valid id", async () => {
    (api.api.getJob as any).mockResolvedValue(mockJob);

    const { result } = renderHook(() => useJob("job-1"));

    expect(result.current.loading).toBe(true);

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.job).toEqual(mockJob);
    expect(result.current.error).toBeNull();
    expect(api.api.getJob).toHaveBeenCalledWith("job-1");
  });

  it("does not fetch if id is empty", async () => {
    (api.api.getJob as any).mockResolvedValue(mockJob);

    const { result } = renderHook(() => useJob(""));

    // Should still be loading since refresh was not called due to empty id
    expect(result.current.loading).toBe(true);
    expect(result.current.job).toBeNull();
    expect(api.api.getJob).not.toHaveBeenCalled();
  });

  it("handles fetch error", async () => {
    const errorMessage = "Job not found";
    (api.api.getJob as any).mockRejectedValue(new Error(errorMessage));

    const { result } = renderHook(() => useJob("job-1"));

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe(errorMessage);
    expect(result.current.job).toBeNull();
  });

  it("refetches when id changes", async () => {
    const job1 = { ...mockJob, id: "job-1", name: "Job 1" };
    const job2 = { ...mockJob, id: "job-2", name: "Job 2" };

    (api.api.getJob as any).mockResolvedValue(job1);

    const { result, rerender } = renderHook(
      ({ id }: { id: string }) => useJob(id),
      { initialProps: { id: "job-1" } }
    );

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.job).toEqual(job1);
    expect(api.api.getJob).toHaveBeenCalledWith("job-1");

    (api.api.getJob as any).mockResolvedValue(job2);

    rerender({ id: "job-2" });

    await waitFor(() => {
      expect(api.api.getJob).toHaveBeenCalledWith("job-2");
    });

    expect(result.current.job).toEqual(job2);
  });

  it("refresh function refetches the job", async () => {
    (api.api.getJob as any).mockResolvedValue(mockJob);

    const { result } = renderHook(() => useJob("job-1"));

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    const updatedJob = { ...mockJob, name: "Updated Job 1" };
    (api.api.getJob as any).mockClear();
    (api.api.getJob as any).mockResolvedValue(updatedJob);

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.job).toEqual(updatedJob);
    expect(api.api.getJob).toHaveBeenCalledWith("job-1");
  });

  it("clears error on successful fetch", async () => {
    (api.api.getJob as any).mockRejectedValueOnce(new Error("Network error"));

    const { result } = renderHook(() => useJob("job-1"));

    await waitFor(() => {
      expect(result.current.error).toBe("Network error");
    });

    (api.api.getJob as any).mockClear();
    (api.api.getJob as any).mockResolvedValue(mockJob);

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.error).toBeNull();
    expect(result.current.job).toEqual(mockJob);
  });

  it("handles generic error objects", async () => {
    (api.api.getJob as any).mockRejectedValue(new Error("Generic error"));

    const { result } = renderHook(() => useJob("job-1"));

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.error).toBe("Generic error");
  });
});
