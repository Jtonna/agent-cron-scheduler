"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { JobRun } from "@/lib/types";

export function useRuns(jobId: string, limit: number = 20, offset: number = 0) {
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [total, setTotal] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!jobId) return;
    try {
      const data = await api.listRuns(jobId, limit, offset);
      setRuns(data.runs);
      setTotal(data.total);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch runs");
    } finally {
      setLoading(false);
    }
  }, [jobId, limit, offset]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { runs, total, loading, error, refresh };
}
