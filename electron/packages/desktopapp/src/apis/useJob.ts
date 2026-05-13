"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Fetches a single job by id. Cache invalidation on `job_changed` SSE
 * events is handled centrally by SSEQueryBridge.
 */
export function useJob(jobId: string) {
  const query = useQuery({
    queryKey: ["jobs", jobId],
    queryFn: () => api.getJob(jobId),
    enabled: !!jobId,
  });

  // `isLoading` is `isPending && isFetching`; on a freshly-mounted query the
  // fetch hasn't started yet, so `isLoading` is briefly false while `data` is
  // still undefined. Use `isPending` so the caller never sees that gap and
  // mistakes it for "not found".
  return {
    job: query.data ?? null,
    loading: query.isPending,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
