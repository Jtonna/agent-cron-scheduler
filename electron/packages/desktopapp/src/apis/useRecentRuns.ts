"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/apis/client";
import { useSSEEvents } from "@/apis/sse";
import type { RecentRunEntry } from "@/apis/types";

const PAGE_SIZE = 20;

export function useRecentRuns(initialLimit: number = PAGE_SIZE) {
  const [limit, setLimit] = useState(initialLimit);
  const [runs, setRuns] = useState<RecentRunEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [hasMore, setHasMore] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchRuns = useCallback(async (currentLimit: number, isLoadMore: boolean) => {
    if (isLoadMore) {
      setLoadingMore(true);
    } else {
      setLoading(true);
    }
    setError(null);
    try {
      const data = await api.listRecentRuns(currentLimit);
      setRuns(data.runs);
      // If we got fewer runs than requested, we've hit the end
      setHasMore(data.runs.length >= currentLimit);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch recent runs",
      );
    } finally {
      setLoading(false);
      setLoadingMore(false);
    }
  }, []);

  useEffect(() => {
    fetchRuns(limit, false);
  }, [fetchRuns, limit]);

  const loadMore = useCallback(() => {
    if (loadingMore || !hasMore) return;
    setLimit((prev) => prev + PAGE_SIZE);
  }, [loadingMore, hasMore]);

  const refresh = useCallback(() => {
    fetchRuns(limit, false);
  }, [fetchRuns, limit]);

  // Auto-refresh when SSE job events arrive
  useSSEEvents((event) => {
    if (["started", "completed", "failed", "killed"].includes(event.type)) {
      fetchRuns(limit, false);
    }
  });

  return { runs, loading, loadingMore, hasMore, error, refresh, loadMore };
}
