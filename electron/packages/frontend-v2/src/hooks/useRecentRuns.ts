"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { RecentRunEntry } from "@/lib/types";

export function useRecentRuns(limit: number = 12) {
  const [runs, setRuns] = useState<RecentRunEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const data = await api.listRecentRuns(limit);
      setRuns(data.runs);
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch recent runs"
      );
    } finally {
      setLoading(false);
    }
  }, [limit]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { runs, loading, error, refresh };
}
