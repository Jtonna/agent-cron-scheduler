"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Fetches a single run by id. Cache invalidation on `run_started` /
 * `run_completed` / `run_failed` / `step_completed` SSE events is handled
 * centrally by SSEQueryBridge.
 */
export function useRun(runId: string) {
  const query = useQuery({
    queryKey: ["runs", runId],
    queryFn: () => api.getRun(runId),
    enabled: !!runId,
  });

  // `isLoading` is `isPending && isFetching`; on a freshly-mounted query the
  // fetch hasn't started yet, so `isLoading` is briefly false while `data` is
  // still undefined. Use `isPending` so the caller never sees that gap and
  // mistakes it for "not found".
  return {
    run: query.data ?? null,
    loading: query.isPending,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
