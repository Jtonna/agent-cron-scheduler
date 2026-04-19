"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { api } from "@/lib/api";
import type { JobRun } from "@/lib/types";
import { useSSEEvents } from "@/hooks/useSSE";

const PAGE_SIZE = 20;

export function useRuns(jobId: string) {
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [totalCount, setTotalCount] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const offsetRef = useRef(0);
  const abortRef = useRef<AbortController | null>(null);

  const refresh = useCallback(async () => {
    if (!jobId) return;
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    setLoading(true);
    offsetRef.current = 0;
    try {
      const data = await api.listRuns(jobId, PAGE_SIZE, 0);
      if (controller.signal.aborted) return;
      setRuns(data.runs ?? []);
      setTotalCount(data.total ?? 0);
      offsetRef.current = data.runs?.length ?? 0;
      setError(null);
    } catch (err) {
      if (controller.signal.aborted) return;
      setError(err instanceof Error ? err.message : "Failed to fetch runs");
    } finally {
      if (!controller.signal.aborted) setLoading(false);
    }
  }, [jobId]);

  const loadMore = useCallback(async () => {
    if (!jobId) return;
    const currentOffset = offsetRef.current;
    if (currentOffset >= totalCount) return;

    try {
      const data = await api.listRuns(jobId, PAGE_SIZE, currentOffset);
      setRuns((prev) => [...prev, ...(data.runs ?? [])]);
      setTotalCount(data.total ?? 0);
      offsetRef.current = currentOffset + (data.runs?.length ?? 0);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load more runs");
    }
  }, [jobId, totalCount]);

  useEffect(() => {
    if (jobId) refresh();
    return () => {
      abortRef.current?.abort();
    };
  }, [jobId, refresh]);

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "started" ||
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "killed"
        ) {
          try {
            const payload = JSON.parse(event.data);
            if (payload?.job_id === jobId) {
              refresh();
            }
          } catch {
            // ignore parse errors
          }
        }
      },
      [jobId, refresh]
    )
  );

  return { runs, totalCount, loading, error, loadMore, refresh };
}
