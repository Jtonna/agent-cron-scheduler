"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { GlobalCostSummaryResponse } from "@/lib/types";

export function useGlobalCostSummary() {
  const [summary, setSummary] = useState<GlobalCostSummaryResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const result = await api.getGlobalCostSummary();
      // Map daily_trend entries to include 'cost' field from 'cost_usd'
      const mappedResult = {
        ...result,
        daily_trend: result.daily_trend.map((item) => ({
          ...item,
          cost: item.cost_usd,
        })),
      };
      setSummary(mappedResult);
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch global cost summary"
      );
      setSummary(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { summary, loading, error, refresh };
}
