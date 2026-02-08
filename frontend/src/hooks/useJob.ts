"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { Job } from "@/lib/types";

export function useJob(id: string) {
  const [job, setJob] = useState<Job | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const data = await api.getJob(id);
      setJob(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch job");
    } finally {
      setLoading(false);
    }
  }, [id]);

  useEffect(() => {
    if (id) refresh();
  }, [id, refresh]);

  return { job, loading, error, refresh };
}
