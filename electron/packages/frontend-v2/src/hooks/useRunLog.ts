"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import { api } from "@/lib/api";
import { useSSEEvents } from "@/hooks/useSSE";

const POLL_INTERVAL_MS = 2500;

export function useRunLog(jobId: string, runId: string) {
  const [log, setLog] = useState<string>("");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const completedRef = useRef(false);
  const pollTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const fetchLog = useCallback(async () => {
    if (!runId) return;
    try {
      const data = await api.getRunLog(runId);
      setLog(data);
      setError(null);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to fetch run log");
    } finally {
      setLoading(false);
    }
  }, [runId]);

  const schedulePoll = useCallback(() => {
    if (completedRef.current) return;
    pollTimerRef.current = setTimeout(async () => {
      if (completedRef.current) return;
      await fetchLog();
      schedulePoll();
    }, POLL_INTERVAL_MS);
  }, [fetchLog]);

  const stopPolling = useCallback(() => {
    if (pollTimerRef.current !== null) {
      clearTimeout(pollTimerRef.current);
      pollTimerRef.current = null;
    }
  }, []);

  // Initial fetch and start polling
  useEffect(() => {
    if (!runId) return;

    completedRef.current = false;
    abortRef.current = new AbortController();

    setLoading(true);
    fetchLog().then(() => {
      if (!completedRef.current) {
        schedulePoll();
      }
    });

    return () => {
      abortRef.current?.abort();
      stopPolling();
    };
  }, [runId, fetchLog, schedulePoll, stopPolling]);

  // SSE: stream output chunks and detect run completion
  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "output") {
          try {
            const payload = JSON.parse(event.data);
            if (payload?.run_id === runId) {
              const chunk: string = payload.data ?? payload.chunk ?? "";
              if (chunk) {
                setLog((prev) => prev + chunk);
              }
            }
          } catch {
            // ignore parse errors
          }
        } else if (
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "killed"
        ) {
          try {
            const payload = JSON.parse(event.data);
            if (payload?.run_id === runId) {
              completedRef.current = true;
              stopPolling();
              // Final fetch to ensure log is complete
              fetchLog();
            }
          } catch {
            // ignore parse errors
          }
        }
      },
      [runId, fetchLog, stopPolling]
    )
  );

  return { log, loading, error };
}
