import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import type { ReactNode } from "react";
import type { Job } from "./types";

vi.mock("@/apis/client", () => ({
  api: {
    listJobs: vi.fn(),
  },
}));

import { api } from "@/apis/client";
import { useJobs } from "./useJobs";

function makeJob(id: string): Job {
  return {
    id,
    name: `Job ${id}`,
    schedule: "0 * * * *",
    execution: { type: "ShellCommand", value: "echo hi" },
    enabled: true,
    timezone: null,
    working_dir: null,
    env_vars: null,
    timeout_secs: 60,
    log_environment: false,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    last_run_at: null,
    last_exit_code: null,
    next_run_at: null,
  };
}

function makeWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false, gcTime: 0, staleTime: 0 },
    },
  });
  return function Wrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
  };
}

describe("useJobs", () => {
  const mockedListJobs = vi.mocked(api.listJobs);

  beforeEach(() => {
    mockedListJobs.mockReset();
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("starts in a loading state with no data and no error", () => {
    mockedListJobs.mockImplementation(() => new Promise(() => {}));
    const { result } = renderHook(() => useJobs(), { wrapper: makeWrapper() });
    expect(result.current.loading).toBe(true);
    expect(result.current.jobs).toEqual([]);
    expect(result.current.error).toBeNull();
  });

  it("returns the jobs array once the request resolves", async () => {
    const jobs = [makeJob("a"), makeJob("b")];
    mockedListJobs.mockResolvedValueOnce(jobs);

    const { result } = renderHook(() => useJobs(), { wrapper: makeWrapper() });

    await waitFor(() => {
      expect(result.current.loading).toBe(false);
    });

    expect(result.current.jobs).toEqual(jobs);
    expect(result.current.error).toBeNull();
  });

  it("surfaces fetch errors as a string message", async () => {
    mockedListJobs.mockRejectedValueOnce(new Error("network down"));

    const { result } = renderHook(() => useJobs(), { wrapper: makeWrapper() });

    await waitFor(() => {
      expect(result.current.error).toBe("network down");
    });
    expect(result.current.loading).toBe(false);
    expect(result.current.jobs).toEqual([]);
  });
});
