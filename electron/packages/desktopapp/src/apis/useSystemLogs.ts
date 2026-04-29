"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/apis/client";

export function useSystemLogs(tail: number = 500) {
  const [logs, setLogs] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const fetchLogs = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const text = await api.getSystemLogs(tail);
      setLogs(text);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch logs");
    } finally {
      setLoading(false);
    }
  }, [tail]);

  useEffect(() => {
    fetchLogs();
  }, [fetchLogs]);

  return { logs, loading, error, refresh: fetchLogs, setLogs };
}
