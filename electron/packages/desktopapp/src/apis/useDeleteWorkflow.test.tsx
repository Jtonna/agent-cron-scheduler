import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, waitFor, act } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";

vi.mock("@/apis/client", async () => {
  const actual = await vi.importActual<typeof import("@/apis/client")>("@/apis/client");
  return {
    ...actual,
    api: {
      deleteWorkflow: vi.fn(),
    },
  };
});

import { api, ApiError } from "@/apis/client";
import { useDeleteWorkflow } from "./useDeleteWorkflow";

const mockedDeleteWorkflow = vi.mocked(api.deleteWorkflow);

function makeWrapper() {
  // Note: gcTime is left at its default (not 0) here, unlike other hook
  // tests in this file tree — these tests seed cache entries with
  // `setQueryData` and assert on them after the mutation settles, and an
  // unobserved query with gcTime: 0 is garbage-collected immediately.
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, staleTime: 0 },
    },
  });
  const wrapper = function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
  return { queryClient, wrapper };
}

describe("useDeleteWorkflow", () => {
  beforeEach(() => {
    mockedDeleteWorkflow.mockReset();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("on success: deletes, calls onDeleted before clearing caches, and updates queries in order", async () => {
    let resolveDelete!: () => void;
    mockedDeleteWorkflow.mockImplementationOnce(
      () =>
        new Promise<void>((resolve) => {
          resolveDelete = resolve;
        })
    );
    const { queryClient, wrapper } = makeWrapper();

    queryClient.setQueryData(["jobs", "wf-1"], { id: "wf-1" });

    const removeQueriesSpy = vi.spyOn(queryClient, "removeQueries");
    const invalidateQueriesSpy = vi.spyOn(queryClient, "invalidateQueries");
    const onDeleted = vi.fn();

    const { result } = renderHook(() => useDeleteWorkflow({ onDeleted }), { wrapper });

    expect(result.current.deleting).toBe(false);

    let promise!: Promise<void>;
    act(() => {
      promise = result.current.deleteWorkflow("wf-1");
    });

    await waitFor(() => {
      expect(result.current.deleting).toBe(true);
    });

    await act(async () => {
      resolveDelete();
      await promise;
    });

    await waitFor(() => {
      expect(result.current.deleting).toBe(false);
    });

    expect(result.current.error).toBeNull();
    expect(onDeleted).toHaveBeenCalledWith("wf-1");

    // onDeleted must fire before the caches are torn down.
    const onDeletedOrder = onDeleted.mock.invocationCallOrder[0];
    const removeQueriesOrder = removeQueriesSpy.mock.invocationCallOrder[0];
    expect(onDeletedOrder).toBeLessThan(removeQueriesOrder);

    expect(removeQueriesSpy).toHaveBeenCalledWith({ queryKey: ["jobs", "wf-1"] });
    expect(removeQueriesSpy).toHaveBeenCalledWith({
      queryKey: ["workflows", "wf-1", "cost"],
    });
    expect(invalidateQueriesSpy).toHaveBeenCalledWith({
      queryKey: ["jobs"],
      exact: true,
    });
    expect(invalidateQueriesSpy).toHaveBeenCalledWith({ queryKey: ["cost/workflows"] });

    expect(queryClient.getQueryData(["jobs", "wf-1"])).toBeUndefined();
  });

  it("surfaces a 404 as errorStatus", async () => {
    mockedDeleteWorkflow.mockRejectedValueOnce(new ApiError(404, "not_found", "Workflow not found"));
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useDeleteWorkflow(), { wrapper });

    await act(async () => {
      await expect(result.current.deleteWorkflow("wf-missing")).rejects.toThrow();
    });

    await waitFor(() => {
      expect(result.current.errorStatus).toBe(404);
    });
  });

  it("on 409: surfaces the server message and code, leaves cache untouched, and does not call onDeleted", async () => {
    const conflictError = new ApiError(409, "workflow_run_active", "Workflow has an active run");
    mockedDeleteWorkflow.mockRejectedValueOnce(conflictError);
    const { queryClient, wrapper } = makeWrapper();

    queryClient.setQueryData(["jobs", "wf-2"], { id: "wf-2" });
    const onDeleted = vi.fn();

    const { result } = renderHook(() => useDeleteWorkflow({ onDeleted }), { wrapper });

    await act(async () => {
      await expect(result.current.deleteWorkflow("wf-2")).rejects.toThrow();
    });

    await waitFor(() => {
      expect(result.current.error).toBe("Workflow has an active run");
    });

    expect(result.current.errorCode).toBe("workflow_run_active");
    expect(onDeleted).not.toHaveBeenCalled();
    expect(queryClient.getQueryData(["jobs", "wf-2"])).toEqual({ id: "wf-2" });
  });

  it("reset clears the error", async () => {
    mockedDeleteWorkflow.mockRejectedValueOnce(new ApiError(500, "internal_error", "boom"));
    const { wrapper } = makeWrapper();

    const { result } = renderHook(() => useDeleteWorkflow(), { wrapper });

    await act(async () => {
      await expect(result.current.deleteWorkflow("wf-3")).rejects.toThrow();
    });

    await waitFor(() => {
      expect(result.current.error).toBe("boom");
    });

    act(() => {
      result.current.reset();
    });

    await waitFor(() => {
      expect(result.current.error).toBeNull();
    });
    expect(result.current.errorCode).toBeNull();
    expect(result.current.errorStatus).toBeNull();
  });
});
