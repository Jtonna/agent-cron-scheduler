"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { api } from "@/lib/api";

export function useRunLog(runId: string | null, pollInterval?: number) {
  const [log, setLog] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const refresh = useCallback(async () => {
    if (!runId) return;
    setLoading(true);
    try {
      const data = await api.getRunLog(runId);
      setLog(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch log");
    } finally {
      setLoading(false);
    }
  }, [runId]);

  // Initial fetch
  useEffect(() => {
    if (runId) refresh();
  }, [runId, refresh]);

  // Polling
  useEffect(() => {
    if (intervalRef.current) {
      clearInterval(intervalRef.current);
      intervalRef.current = null;
    }

    if (pollInterval && pollInterval > 0 && runId) {
      intervalRef.current = setInterval(() => {
        refresh();
      }, pollInterval);
    }

    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [pollInterval, runId, refresh]);

  return { log, loading, error, refresh };
}
