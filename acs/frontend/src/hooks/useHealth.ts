"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { HealthResponse } from "@/lib/types";

export function useHealth(intervalMs: number = 5000) {
  const [health, setHealth] = useState<HealthResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchHealth = useCallback(async () => {
    try {
      const data = await api.health();
      setHealth(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch health");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchHealth();
    const timer = setInterval(fetchHealth, intervalMs);
    return () => clearInterval(timer);
  }, [fetchHealth, intervalMs]);

  return { health, loading, error, refresh: fetchHealth };
}
