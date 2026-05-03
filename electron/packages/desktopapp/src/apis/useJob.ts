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

  return {
    job: query.data ?? null,
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
