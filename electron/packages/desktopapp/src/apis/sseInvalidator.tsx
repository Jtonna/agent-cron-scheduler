"use client";

import { useQueryClient } from "@tanstack/react-query";
import { useSSEEvents } from "./sse";

/**
 * Single, app-wide bridge between SSE events and React Query cache.
 *
 * Mounted once at the provider level (not per-hook). Listens to every SSE
 * event and dispatches the appropriate `invalidateQueries` calls so that
 * any active query refetches as state on the daemon changes.
 *
 * This replaces having every hook independently subscribe to SSE — that
 * pattern multiplied the subscriber count by however many hooks were
 * mounted, and each subscriber paid the cost of every event regardless of
 * whether it cared.
 */
export function SSEQueryBridge() {
  const queryClient = useQueryClient();

  useSSEEvents((event) => {
    switch (event.type) {
      case "job_changed":
        queryClient.invalidateQueries({ queryKey: ["jobs"] });
        break;

      case "started":
      case "completed":
      case "failed":
      case "killed":
        // Recent runs feed and global cost summary both depend on run state.
        queryClient.invalidateQueries({ queryKey: ["runs/recent"] });
        queryClient.invalidateQueries({ queryKey: ["cost/workflows"] });
        // Per-job run lists — if the event has a job_id, only invalidate that
        // job's runs; otherwise be conservative and invalidate all of them.
        try {
          const parsed = JSON.parse(event.data);
          const eventJobId =
            parsed && typeof parsed === "object" ? (parsed.job_id ?? parsed.jobId) : undefined;
          if (eventJobId) {
            queryClient.invalidateQueries({
              queryKey: ["jobs", eventJobId, "runs"],
            });
          } else {
            queryClient.invalidateQueries({ queryKey: ["jobs"], exact: false });
          }
        } catch {
          queryClient.invalidateQueries({ queryKey: ["jobs"], exact: false });
        }
        break;

      // `output` events stream into the systemlogs cache and are handled
      // inline by useSystemLogs — they're high frequency, so we don't
      // touch them here.
      default:
        break;
    }
  });

  return null;
}
