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
      // Workflow definition lifecycle (created/updated/deleted/enabled/disabled).
      // v3 equivalent: "job_changed".
      case "workflow_changed":
        queryClient.invalidateQueries({ queryKey: ["jobs"] });
        break;

      // Run lifecycle. v4 has no separate "killed" event — killed runs surface
      // as run_completed with status: "Killed". v3 equivalents:
      //   started → run_started
      //   completed/killed → run_completed
      //   failed → run_failed
      case "run_started":
      case "run_completed":
      case "run_failed": {
        // Recent runs feed and global cost summary both depend on run state.
        queryClient.invalidateQueries({ queryKey: ["runs/recent"] });
        queryClient.invalidateQueries({ queryKey: ["cost/workflows"] });
        // Per-job run lists — if the event has a workflow_id (v4) or job_id
        // (legacy fallback), only invalidate that workflow's runs; otherwise
        // be conservative and invalidate all of them.
        try {
          const parsed = JSON.parse(event.data);
          const eventJobId =
            parsed && typeof parsed === "object"
              ? (parsed.workflow_id ?? parsed.workflowId ?? parsed.job_id ?? parsed.jobId)
              : undefined;
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
      }

      // `step_output` events stream into the systemlogs cache and are handled
      // inline by useSystemLogs — they're high frequency, so we don't
      // touch them here.
      //
      // `step_started` / `step_completed` don't drive any list-level queries
      // today; per-run detail views can subscribe directly via useSSEEvents
      // if/when they need step-granularity updates.
      default:
        break;
    }
  });

  return null;
}
