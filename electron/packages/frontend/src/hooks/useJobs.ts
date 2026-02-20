"use client";

import { useState, useEffect, useCallback } from "react";
import { api } from "@/lib/api";
import type { Job } from "@/lib/types";

export function useJobs() {
  const [jobs, setJobs] = useState<Job[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const data = await api.listJobs();
      setJobs(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch jobs");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const triggerJob = useCallback(
    async (id: string) => {
      await api.triggerJob(id);
      await refresh();
    },
    [refresh]
  );

  const enableJob = useCallback(
    async (id: string) => {
      await api.enableJob(id);
      await refresh();
    },
    [refresh]
  );

  const disableJob = useCallback(
    async (id: string) => {
      await api.disableJob(id);
      await refresh();
    },
    [refresh]
  );

  const deleteJob = useCallback(
    async (id: string) => {
      await api.deleteJob(id);
      await refresh();
    },
    [refresh]
  );

  return {
    jobs,
    loading,
    error,
    refresh,
    triggerJob,
    enableJob,
    disableJob,
    deleteJob,
  };
}
