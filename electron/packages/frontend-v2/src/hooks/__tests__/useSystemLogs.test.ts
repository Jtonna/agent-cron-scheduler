import { renderHook, waitFor, act } from "@testing-library/react";
import { vi, describe, it, expect, beforeEach } from "vitest";

vi.mock("@/lib/api", () => ({
  api: {
    getSystemLogs: vi.fn(),
  },
}));

import { api } from "@/lib/api";
import { useSystemLogs } from "../useSystemLogs";

const mockApi = api as { getSystemLogs: ReturnType<typeof vi.fn> };

const mockLogs = "2024-01-01 INFO: Service started\n2024-01-01 INFO: Job scheduled";

describe("useSystemLogs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("starts with loading true", () => {
    mockApi.getSystemLogs.mockResolvedValue(mockLogs);
    const { result } = renderHook(() => useSystemLogs());
    expect(result.current.loading).toBe(true);
  });

  it("returns logs string on successful fetch", async () => {
    mockApi.getSystemLogs.mockResolvedValue(mockLogs);
    const { result } = renderHook(() => useSystemLogs());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.logs).toBe(mockLogs);
    expect(result.current.error).toBeNull();
  });

  it("respects tail parameter", async () => {
    mockApi.getSystemLogs.mockResolvedValue(mockLogs);
    renderHook(() => useSystemLogs(50));

    await waitFor(() => expect(mockApi.getSystemLogs).toHaveBeenCalledWith(50));
  });

  it("uses default tail of 100", async () => {
    mockApi.getSystemLogs.mockResolvedValue(mockLogs);
    renderHook(() => useSystemLogs());

    await waitFor(() => expect(mockApi.getSystemLogs).toHaveBeenCalledWith(100));
  });

  it("sets error on API failure", async () => {
    mockApi.getSystemLogs.mockRejectedValue(new Error("Failed to load logs"));
    const { result } = renderHook(() => useSystemLogs());

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.error).toBe("Failed to load logs");
    expect(result.current.logs).toBe("");
  });

  it("refresh triggers re-fetch", async () => {
    mockApi.getSystemLogs.mockResolvedValue(mockLogs);
    const { result } = renderHook(() => useSystemLogs());

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(mockApi.getSystemLogs).toHaveBeenCalledTimes(1);

    const newLogs = mockLogs + "\n2024-01-01 INFO: Additional log entry";
    mockApi.getSystemLogs.mockResolvedValue(newLogs);

    await act(async () => {
      await result.current.refresh();
    });

    expect(mockApi.getSystemLogs).toHaveBeenCalledTimes(2);
    expect(result.current.logs).toBe(newLogs);
  });
});
