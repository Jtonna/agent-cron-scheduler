import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

vi.mock("@/apis/client", async () => {
  const actual = await vi.importActual<typeof import("@/apis/client")>("@/apis/client");
  return {
    ...actual,
    api: {
      ...actual.api,
      triggerWorkflow: vi.fn(),
    },
  };
});

const push = vi.hoisted(() => vi.fn());
const router = { push, replace: vi.fn() };
vi.mock("next/navigation", () => ({
  useRouter: () => router,
}));

import { api, ApiError } from "@/apis/client";
import { useRunWorkflowAndOpen } from "./useRunWorkflowAndOpen";

const mockedTriggerWorkflow = vi.mocked(api.triggerWorkflow);

function makeWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0 },
      mutations: { retry: false },
    },
  });
  const wrapper = function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
  return { queryClient, wrapper };
}

describe("useRunWorkflowAndOpen", () => {
  beforeEach(() => {
    mockedTriggerWorkflow.mockReset();
    push.mockReset();
    router.replace.mockReset();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("on success: calls trigger and pushes to the run page", async () => {
    let resolveTrigger!: (
      value: {
        run_id: string;
        workflow_id: string;
        workflow_version: number;
        run_url: string;
      }
    ) => void;
    mockedTriggerWorkflow.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolveTrigger = resolve;
        })
    );
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    expect(result.current.running).toBe(false);
    expect(result.current.error).toBeNull();
    expect(result.current.activeRunId).toBeNull();

    let promise!: Promise<void>;
    act(() => {
      promise = result.current.run();
    });

    await waitFor(() => {
      expect(result.current.running).toBe(true);
    });

    await act(async () => {
      resolveTrigger({
        run_id: "r1",
        workflow_id: "w1",
        workflow_version: 1,
        run_url: "/runs/r1",
      });
      await promise;
    });

    await waitFor(() => {
      expect(result.current.running).toBe(false);
    });

    expect(result.current.error).toBeNull();
    expect(result.current.activeRunId).toBeNull();
    expect(push).toHaveBeenCalledWith("/workflows/w1/runs/r1");
    expect(mockedTriggerWorkflow).toHaveBeenCalledWith("w1", {});
  });

  it("on 409 concurrent_run_active: sets activeRunId and does not push", async () => {
    const conflictError = new ApiError(
      409,
      "concurrent_run_active",
      "A run is already active",
      { active_run_id: "r-9" }
    );
    let rejectTrigger!: (reason?: unknown) => void;
    mockedTriggerWorkflow.mockImplementationOnce(
      () =>
        new Promise((_, reject) => {
          rejectTrigger = reject;
        })
    );
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    let promise!: Promise<void>;
    act(() => {
      promise = result.current.run();
    });

    await waitFor(() => {
      expect(result.current.running).toBe(true);
    });

    await act(async () => {
      rejectTrigger(conflictError);
      await promise;
    });

    await waitFor(() => {
      expect(result.current.running).toBe(false);
    });

    expect(result.current.error).toBe("A run is already active");
    expect(result.current.activeRunId).toBe("r-9");
    expect(push).not.toHaveBeenCalled();
  });

  it("on other API error (500): sets error and does not push", async () => {
    mockedTriggerWorkflow.mockRejectedValueOnce(
      new ApiError(500, "internal_error", "Server error")
    );
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    let promise!: Promise<void>;
    act(() => {
      promise = result.current.run();
    });

    await act(async () => {
      await promise;
    });

    await waitFor(() => {
      expect(result.current.running).toBe(false);
    });

    expect(result.current.error).toBe("Server error");
    expect(result.current.activeRunId).toBeNull();
    expect(push).not.toHaveBeenCalled();
  });

  it("reset clears error and activeRunId", async () => {
    mockedTriggerWorkflow.mockRejectedValueOnce(
      new ApiError(500, "internal_error", "boom")
    );
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    let promise!: Promise<void>;
    act(() => {
      promise = result.current.run();
    });

    await act(async () => {
      await promise;
    });

    await waitFor(() => {
      expect(result.current.error).toBe("boom");
    });

    act(() => {
      result.current.reset();
    });

    await waitFor(() => {
      expect(result.current.error).toBeNull();
      expect(result.current.activeRunId).toBeNull();
    });
  });

  it("on failure then success: clears error and pushes", async () => {
    mockedTriggerWorkflow.mockRejectedValueOnce(
      new ApiError(500, "internal_error", "first error")
    );
    mockedTriggerWorkflow.mockResolvedValueOnce({
      run_id: "r2",
      workflow_id: "w1",
      workflow_version: 1,
      run_url: "/runs/r2",
    });
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    // First call fails
    let promise!: Promise<void>;
    act(() => {
      promise = result.current.run();
    });

    await act(async () => {
      await promise;
    });

    await waitFor(() => {
      expect(result.current.error).toBe("first error");
    });

    // Second call succeeds
    act(() => {
      promise = result.current.run();
    });

    await act(async () => {
      await promise;
    });

    await waitFor(() => {
      expect(result.current.running).toBe(false);
    });

    expect(result.current.error).toBeNull();
    expect(push).toHaveBeenCalledWith("/workflows/w1/runs/r2");
  });

  it("multiple rapid calls: only triggers once", async () => {
    mockedTriggerWorkflow.mockResolvedValueOnce({
      run_id: "r1",
      workflow_id: "w1",
      workflow_version: 1,
      run_url: "/runs/r1",
    });
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    let promise1!: Promise<void>;
    let promise2!: Promise<void>;
    let promise3!: Promise<void>;

    act(() => {
      promise1 = result.current.run();
      promise2 = result.current.run();
      promise3 = result.current.run();
    });

    await act(async () => {
      await Promise.all([promise1, promise2, promise3]);
    });

    expect(mockedTriggerWorkflow).toHaveBeenCalledTimes(1);
  });

  it("openRun navigates to the run page", () => {
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useRunWorkflowAndOpen("w1"), { wrapper });

    result.current.openRun("r5");

    expect(push).toHaveBeenCalledWith("/workflows/w1/runs/r5");
  });
});
