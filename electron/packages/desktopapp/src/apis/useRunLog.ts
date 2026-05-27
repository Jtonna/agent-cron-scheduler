"use client";

import { useQuery, useQueryClient } from "@tanstack/react-query";
import { api } from "@/apis/client";
import { useSSEEvents } from "@/apis/sse";

// Hard cap on the in-memory log buffer to prevent unbounded growth from
// long-lived SSE streams. ~1MB is plenty for the last several thousand lines.
const MAX_LOG_BUFFER_CHARS = 1_000_000;

function capLogBuffer(buffer: string): string {
  if (buffer.length <= MAX_LOG_BUFFER_CHARS) return buffer;
  // Trim from the front, keep the last MAX_LOG_BUFFER_CHARS, and align to a
  // newline so we don't show a half-truncated line.
  const trimmed = buffer.slice(buffer.length - MAX_LOG_BUFFER_CHARS);
  const newlineIdx = trimmed.indexOf("\n");
  return newlineIdx >= 0 ? trimmed.slice(newlineIdx + 1) : trimmed;
}

/**
 * Fetches the combined log file for a single run and live-tails new chunks
 * streamed via SSE `step_output` events whose `run_id` matches. For runs
 * already finished the backend serves a static file and no SSE events fire,
 * which is the desired behavior. Buffer is capped at ~1MB to prevent
 * unbounded growth on long-running jobs.
 */
export function useRunLog(runId: string) {
  const queryClient = useQueryClient();
  const queryKey = ["runs", runId, "log"];

  const query = useQuery({
    queryKey,
    queryFn: () => api.getRunLog(runId),
    enabled: !!runId,
  });

  useSSEEvents((event) => {
    if (event.type !== "step_output") return;
    let chunk = "";
    try {
      const parsed = JSON.parse(event.data);
      if (parsed && typeof parsed === "object" && parsed.run_id === runId && typeof parsed.data === "string") {
        chunk = parsed.data;
      }
    } catch {
      // ignore unparseable payloads
    }
    if (!chunk) return;
    queryClient.setQueryData<string>(queryKey, (old) => {
      const prev = old ?? "";
      const next = prev ? prev + "\n" + chunk : chunk;
      return capLogBuffer(next);
    });
  });

  return {
    logs: query.data ?? "",
    loading: query.isPending,
    error: query.error instanceof Error ? query.error.message : null,
    refresh: query.refetch,
  };
}
