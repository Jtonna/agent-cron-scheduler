"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { CostSummaryResponse } from "@/lib/types";

export function useCostSummary(jobId: string) {
  const [costSummary, setCostSummary] = useState<CostSummaryResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [timeframe, setTimeframe] = useState<string>("30d");

  const refresh = useCallback(async () => {
    if (!jobId) return;
    setLoading(true);
    try {
      const data = await api.getCostSummary(jobId, timeframe);
      setCostSummary(data);
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch cost summary"
      );
    } finally {
      setLoading(false);
    }
  }, [jobId, timeframe]);

  useEffect(() => {
    if (jobId) refresh();
  }, [jobId, refresh]);

  return { costSummary, loading, error, setTimeframe };
}
