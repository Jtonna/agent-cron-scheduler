"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Fetches the global cost summary. Cache invalidation on run-completion
 * SSE events is handled centrally by SSEQueryBridge.
 */
export function useGlobalCostSummary(timeframe: string = "30d") {
  const query = useQuery({
    queryKey: ["costs/summary", timeframe],
    queryFn: () => api.getGlobalCostSummary(timeframe),
  });

  return {
    summary: query.data ?? null,
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
