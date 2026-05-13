"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";
import type { WorkflowCostEntry } from "@/apis/types";

/**
 * Fetches the per-workflow cost summary (v4 `/api/cost/workflows/{id}`).
 * Cache invalidation on run-completion SSE events is handled centrally
 * by SSEQueryBridge.
 *
 * The endpoint always returns a 30-day window, so there is no timeframe
 * parameter — last-year totals come back in the same payload.
 */
export function useJobCostSummary(jobId: string) {
  const query = useQuery({
    queryKey: ["workflows", jobId, "cost"],
    queryFn: () => api.getWorkflowCost(jobId),
    enabled: !!jobId,
  });

  return {
    summary: (query.data ?? null) as WorkflowCostEntry | null,
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
