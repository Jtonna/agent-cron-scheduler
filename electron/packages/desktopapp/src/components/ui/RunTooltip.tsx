"use client";

import { createPortal } from "react-dom";
import type { JobRun } from "@/apis/types";
import { JobStateIndicator, apiStatusToJobState } from "./JobStateIndicator";

/**
 * RunTooltip
 *
 * Floating tooltip rendered in a portal that shows a job run's status,
 * timestamp, ID, and (optionally) cost. Anchored via fixed `left`/`top`
 * coords so callers can position it relative to whatever element
 * triggered the hover. Used by the sidebar (anchored to the right edge
 * of the sidebar) and the jobs table (anchored below the hovered dot).
 */

/**
 * Minimal shape needed to render the tooltip body. Both `JobRun` and
 * `RecentRunEntry` satisfy this.
 */
interface RunLike {
  run_id: string;
  status: JobRun["status"];
  started_at: string;
  total_cost_usd?: number | null;
}

interface RunTooltipProps {
  run: RunLike;
  jobName: string;
  /**
   * CSS `left` for the tooltip anchor. Accepts a number (px) or a
   * `calc()` expression string.
   */
  left: number | string;
  /** CSS `top` (px) for the tooltip anchor. */
  top: number;
  /**
   * Transform applied to the tooltip — typically `translateY(-50%)`
   * when anchoring against a vertical center, or `translateX(-50%)`
   * when centering horizontally below an element.
   */
  transform?: string;
}

function shortRunId(runId: string): string {
  return runId.slice(0, 8);
}

function formatTimestamp(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function formatCost(cost: number | null | undefined): string | null {
  if (cost == null || cost === 0) return null;
  return `$${cost.toFixed(3)}`;
}

export function RunTooltip({ run, jobName, left, top, transform }: RunTooltipProps) {
  // SSR-safe portal guard: skip rendering on the server where `document` is undefined.
  // Component is "use client" but Next.js still pre-renders client components in SSR.
  if (typeof document === "undefined") return null;

  const state = apiStatusToJobState(run.status);
  const cost = formatCost(run.total_cost_usd);

  return createPortal(
    <div
      role="tooltip"
      style={{
        position: "fixed",
        left,
        top,
        transform,
        zIndex: 50,
      }}
      className="pointer-events-none bg-fg text-surface text-xs px-3 py-2 rounded-input shadow-menu min-w-[220px]"
    >
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between gap-3">
          <JobStateIndicator state={state} variant="label" />
          <span className="text-fg-faint truncate">{jobName}</span>
        </div>
        <div className="h-px bg-fg-secondary" />
        <div className="flex items-center justify-between gap-3 text-fg-faint">
          <span>Started</span>
          <span className="font-mono text-surface">{formatTimestamp(run.started_at)}</span>
        </div>
        <div className="flex items-center justify-between gap-3 text-fg-faint">
          <span>Run ID</span>
          <span className="font-mono text-surface">{shortRunId(run.run_id)}</span>
        </div>
        {cost && (
          <div className="flex items-center justify-between gap-3 text-fg-faint">
            <span>Cost</span>
            <span className="font-mono text-surface">{cost}</span>
          </div>
        )}
      </div>
    </div>,
    document.body,
  );
}
