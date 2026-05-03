"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Fetches the per-job cost summary. Cache invalidation on run-completion
 * SSE events is handled centrally by SSEQueryBridge.
 */
export function useJobCostSummary(jobId: string, timeframe: string = "30d") {
  const query = useQuery({
    queryKey: ["jobs", jobId, "cost-summary", timeframe],
    queryFn: () => api.getJobCostSummary(jobId, timeframe),
    enabled: !!jobId,
  });

  return {
    summary: query.data ?? null,
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
