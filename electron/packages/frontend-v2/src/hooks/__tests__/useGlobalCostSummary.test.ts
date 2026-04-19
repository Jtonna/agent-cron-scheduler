import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    getGlobalCostSummary: vi.fn(),
  },
}));

import { api } from "@/lib/api";
import { useGlobalCostSummary } from "../useGlobalCostSummary";

const mockApi = api as { getGlobalCostSummary: ReturnType<typeof vi.fn> };

const mockSummary = {
  timeframe: "30d",
  today_usd: 0.42,
  week_usd: 2.1,
  month_usd: 8.75,
  today_tokens: { input: 5000, output: 2000 },
  top_jobs: [
    { job_id: "job-1", job_name: "Daily Report", total_cost: 3.5, total_runs: 30 },
  ],
  daily_trend: [
    { date: "2024-01-01", cost_usd: 0.3, input_tokens: 4000, output_tokens: 1500 },
  ],
};

describe("useGlobalCostSummary", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with loading true", () => {
    mockApi.getGlobalCostSummary.mockResolvedValue(mockSummary);
    const { result } = renderHook(() => useGlobalCostSummary());
    expect(result.current.loading).toBe(true);
  });

  it("returns summary on successful fetch", async () => {
    mockApi.getGlobalCostSummary.mockResolvedValue(mockSummary);
    const { result } = renderHook(() => useGlobalCostSummary());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.summary).toEqual(mockSummary);
    expect(result.current.error).toBeNull();
  });

  it("sets error on API failure", async () => {
    mockApi.getGlobalCostSummary.mockRejectedValue(new Error("Cost summary unavailable"));
    const { result } = renderHook(() => useGlobalCostSummary());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe("Cost summary unavailable");
    expect(result.current.summary).toBeNull();
  });

  it("clears summary on API failure", async () => {
    // First load succeeds
    mockApi.getGlobalCostSummary.mockResolvedValue(mockSummary);
    const { result } = renderHook(() => useGlobalCostSummary());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.summary).toEqual(mockSummary);

    // Then refresh fails
    mockApi.getGlobalCostSummary.mockRejectedValue(new Error("Server error"));

    await act(async () => {
      await result.current.refresh();
    });

    expect(result.current.summary).toBeNull();
    expect(result.current.error).toBe("Server error");
  });

  it("refresh triggers re-fetch", async () => {
    mockApi.getGlobalCostSummary.mockResolvedValue(mockSummary);
    const { result } = renderHook(() => useGlobalCostSummary());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(mockApi.getGlobalCostSummary).toHaveBeenCalledTimes(1);

    const updatedSummary = { ...mockSummary, today_usd: 0.99 };
    mockApi.getGlobalCostSummary.mockResolvedValue(updatedSummary);

    await act(async () => {
      await result.current.refresh();
    });

    expect(mockApi.getGlobalCostSummary).toHaveBeenCalledTimes(2);
    expect(result.current.summary).toEqual(updatedSummary);
  });
});
