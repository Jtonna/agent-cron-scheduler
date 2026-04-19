import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    listRecentRuns: vi.fn(),
  },
}));

import { api } from "@/lib/api";
import { useRecentRuns } from "../useRecentRuns";

const mockApi = api as { listRecentRuns: ReturnType<typeof vi.fn> };

const mockRuns = [
  {
    run_id: "run-1",
    job_id: "job-1",
    job_name: "Daily Backup",
    started_at: "2024-01-01T10:00:00Z",
    finished_at: "2024-01-01T10:05:00Z",
    status: "Completed" as const,
    exit_code: 0,
    log_size_bytes: 1024,
    error: null,
  },
  {
    run_id: "run-2",
    job_id: "job-2",
    job_name: "Weekly Report",
    started_at: "2024-01-01T09:00:00Z",
    finished_at: "2024-01-01T09:10:00Z",
    status: "Failed" as const,
    exit_code: 1,
    log_size_bytes: 512,
    error: "Command failed",
  },
];

const mockResponse = { runs: mockRuns, limit: 12 };

describe("useRecentRuns", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with loading true", () => {
    mockApi.listRecentRuns.mockResolvedValue(mockResponse);
    const { result } = renderHook(() => useRecentRuns());
    expect(result.current.loading).toBe(true);
  });

  it("returns runs array on successful fetch", async () => {
    mockApi.listRecentRuns.mockResolvedValue(mockResponse);
    const { result } = renderHook(() => useRecentRuns());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.runs).toEqual(mockRuns);
    expect(result.current.error).toBeNull();
  });

  it("respects limit parameter", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: mockRuns.slice(0, 1), limit: 5 });
    renderHook(() => useRecentRuns(5));

    await waitFor(() => expect(mockApi.listRecentRuns).toHaveBeenCalledWith(5));
  });

  it("uses default limit of 12", async () => {
    mockApi.listRecentRuns.mockResolvedValue(mockResponse);
    renderHook(() => useRecentRuns());

    await waitFor(() => expect(mockApi.listRecentRuns).toHaveBeenCalledWith(12));
  });

  it("sets error on API failure", async () => {
    mockApi.listRecentRuns.mockRejectedValue(new Error("Failed to fetch recent runs"));
    const { result } = renderHook(() => useRecentRuns());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe("Failed to fetch recent runs");
    expect(result.current.runs).toEqual([]);
  });

  it("refresh triggers re-fetch", async () => {
    mockApi.listRecentRuns.mockResolvedValue(mockResponse);
    const { result } = renderHook(() => useRecentRuns());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(mockApi.listRecentRuns).toHaveBeenCalledTimes(1);

    const newRun = {
      run_id: "run-3",
      job_id: "job-3",
      job_name: "New Job",
      started_at: "2024-01-02T10:00:00Z",
      finished_at: null,
      status: "Running" as const,
      exit_code: null,
      log_size_bytes: 0,
      error: null,
    };
    const updatedResponse = { runs: [newRun, ...mockRuns], limit: 12 };
    mockApi.listRecentRuns.mockResolvedValue(updatedResponse);

    await act(async () => {
      await result.current.refresh();
    });

    expect(mockApi.listRecentRuns).toHaveBeenCalledTimes(2);
    expect(result.current.runs).toEqual(updatedResponse.runs);
  });
});
