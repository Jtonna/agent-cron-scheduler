"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Fetches /api/jobs. Cache invalidation on `job_changed` SSE events is
 * handled centrally by SSEQueryBridge.
 */
export function useJobs() {
  const query = useQuery({
    queryKey: ["jobs"],
    queryFn: () => api.listJobs(),
  });

  return {
    jobs: query.data ?? [],
    loading: query.isLoading,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
