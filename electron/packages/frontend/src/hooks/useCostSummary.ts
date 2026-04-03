"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { CostSummaryResponse } from "@/lib/types";

export function useCostSummary(
  jobId: string,
  timeframe?: string,
  startDate?: string,
  endDate?: string
) {
  const [data, setData] = useState<CostSummaryResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!jobId) return;
    try {
      const result = await api.getCostSummary(jobId, timeframe, startDate, endDate);
      setData(result);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch cost summary");
    } finally {
      setLoading(false);
    }
  }, [jobId, timeframe, startDate, endDate]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { data, loading, error, refresh };
}
