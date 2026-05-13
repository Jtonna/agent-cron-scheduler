"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Fetches the most recent runs for a single job. Cache invalidation on
 * matching SSE events is handled centrally by SSEQueryBridge.
 */
export function useJobRuns(jobId: string, limit: number = 20, offset: number = 0) {
  const query = useQuery({
    queryKey: ["jobs", jobId, "runs", { limit, offset }],
    queryFn: () => api.listJobRuns(jobId, limit, offset),
    enabled: !!jobId,
  });

  return {
    runs: query.data?.runs ?? [],
    loading: query.isPending,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
