"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";

export function useSystemLogs(tail: number = 200) {
  const [logs, setLogs] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const data = await api.getSystemLogs(tail);
      setLogs(data);
      setError(null);
    } catch (err) {
      setError(
        err instanceof Error ? err.message : "Failed to fetch system logs"
      );
    } finally {
      setLoading(false);
    }
  }, [tail]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { logs, loading, error, refresh };
}
