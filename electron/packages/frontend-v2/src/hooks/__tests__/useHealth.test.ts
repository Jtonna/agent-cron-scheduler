import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach, afterEach } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    health: vi.fn(),
  },
}));

import { api } from "@/lib/api";
import { useHealth } from "../useHealth";

const mockApi = api as { health: ReturnType<typeof vi.fn> };

const mockHealthData = {
  status: "ok",
  uptime_seconds: 120,
  active_jobs: 2,
  total_jobs: 5,
  version: "1.0.0",
  data_dir: "/data",
};

describe("useHealth", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with loading true", () => {
    mockApi.health.mockResolvedValue(mockHealthData);
    const { result } = renderHook(() => useHealth());
    expect(result.current.loading).toBe(true);
  });

  it("returns health data on successful fetch", async () => {
    mockApi.health.mockResolvedValue(mockHealthData);
    const { result } = renderHook(() => useHealth());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.health).toEqual(mockHealthData);
    expect(result.current.error).toBeNull();
  });

  it("sets error on API failure", async () => {
    mockApi.health.mockRejectedValue(new Error("Network error"));
    const { result } = renderHook(() => useHealth());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe("Network error");
    expect(result.current.health).toBeNull();
  });

  it("refresh triggers re-fetch", async () => {
    mockApi.health.mockResolvedValue(mockHealthData);
    const { result } = renderHook(() => useHealth());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(mockApi.health).toHaveBeenCalledTimes(1);

    const updatedHealth = { ...mockHealthData, uptime_seconds: 200 };
    mockApi.health.mockResolvedValue(updatedHealth);

    await act(async () => {
      await result.current.refresh();
    });

    expect(mockApi.health).toHaveBeenCalledTimes(2);
    expect(result.current.health).toEqual(updatedHealth);
  });

  describe("interval polling", () => {
    beforeEach(() => {
      vi.useFakeTimers();
    });

    afterEach(() => {
      vi.useRealTimers();
    });

    it("sets up interval polling", async () => {
      mockApi.health.mockResolvedValue(mockHealthData);
      renderHook(() => useHealth(1000));

      await act(async () => {
        await Promise.resolve();
      });

      expect(mockApi.health).toHaveBeenCalledTimes(1);

      await act(async () => {
        vi.advanceTimersByTime(1000);
        await Promise.resolve();
      });

      expect(mockApi.health).toHaveBeenCalledTimes(2);

      await act(async () => {
        vi.advanceTimersByTime(1000);
        await Promise.resolve();
      });

      expect(mockApi.health).toHaveBeenCalledTimes(3);
    });

    it("cleans up interval on unmount", async () => {
      mockApi.health.mockResolvedValue(mockHealthData);
      const { unmount } = renderHook(() => useHealth(1000));

      await act(async () => {
        await Promise.resolve();
      });

      expect(mockApi.health).toHaveBeenCalledTimes(1);

      unmount();

      await act(async () => {
        vi.advanceTimersByTime(3000);
        await Promise.resolve();
      });

      // After unmount, no additional calls should be made
      expect(mockApi.health).toHaveBeenCalledTimes(1);
    });
  });
});
