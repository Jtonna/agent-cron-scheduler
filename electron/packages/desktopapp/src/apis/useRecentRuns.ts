"use client";

import { useState, useCallback } from "react";
import { keepPreviousData, useQuery } from "@tanstack/react-query";
import { api } from "@/apis/client";

const PAGE_SIZE = 20;

export function useRecentRuns(initialLimit: number = PAGE_SIZE) {
  const [limit, setLimit] = useState(initialLimit);

  const query = useQuery({
    queryKey: ["runs/recent", limit],
    queryFn: () => api.listRecentRuns(limit),
    placeholderData: keepPreviousData,
  });

  const runs = query.data?.runs ?? [];
  const hasMore = runs.length >= limit;
  const loading = query.isPending;
  const loadingMore = query.isFetching && !query.isPending;

  const loadMore = useCallback(() => {
    if (loadingMore || !hasMore) return;
    setLimit((prev) => prev + PAGE_SIZE);
  }, [loadingMore, hasMore]);

  return {
    runs,
    loading,
    loadingMore,
    hasMore,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
    loadMore,
  };
}
