"use client";

import { useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

export function useGlobalCostSummary() {
  const query = useQuery({
    queryKey: ["cost/workflows"],
    queryFn: () => api.getCostWorkflows(),
  });

  return {
    summary: query.data ?? null,
    loading: query.isPending,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
