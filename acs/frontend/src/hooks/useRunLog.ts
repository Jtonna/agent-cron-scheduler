"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";

export function useRunLog(runId: string | null) {
  const [log, setLog] = useState<string>("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

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

  useEffect(() => {
    if (runId) refresh();
  }, [runId, refresh]);

  return { log, loading, error, refresh };
}
