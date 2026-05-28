"use client";

import Link from "next/link";
import { useState } from "react";
import { formatTimeAgo, formatTimeUntil } from "@/apis/format";
import type { Job, WorkflowCostSummary } from "@/apis/types";
import { isRunning, type AnyRun } from "@/apis/jobStatus";
import {
  JobStateIndicator,
  apiStatusToJobState,
  type JobState,
} from "@/components/ui/JobStateIndicator";
import { RunTooltip } from "@/components/ui/RunTooltip";
import { FavoriteToggle } from "./FavoriteToggle";

/**
 * JobsListRow
 *
 * One row of the jobs table on `/workflows`. Renders, left-to-right:
 *  - the favorite star (always at the leading edge so the affordance is
 *    consistently discoverable)
 *  - the workflow state dot
 *  - the workflow name
 *  - up to 7 recent run dots (with hover tooltips)
 *  - 30-day cost and 30-day run count (numeric, right-aligned, hidden on
 *    narrow viewports)
 *  - the cron schedule
 *  - last-run time and next-run time
 *
 * The row itself is a navigation link to the workflow detail page; the
 * leading star intercepts its own click so toggling favorite never
 * triggers row navigation.
 *
 * Favorite affordance: when the workflow IS favorited the star is always
 * shown (it doubles as a "this is favorited" indicator); when it is not,
 * the star fades in only on row hover/focus so the row stays uncluttered
 * for the common case.
 */

interface JobsListRowProps {
  job: Job;
  /** Most recent runs for this job, newest first. Up to 7 are rendered. */
  runs?: AnyRun[];
  /**
   * Optional cost rollup for this workflow. When omitted the cost columns
   * render an em-dash. Sourced from the global `/api/cost/workflows`
   * response so the list issues a single network request, not one per
   * row.
   */
  costSummary?: WorkflowCostSummary | null;
}

/**
 * Compute the leading dot's state for a row:
 * - "running" if any recent run is in-flight
 * - "idle" if the job is disabled
 * - The most recent run's status (killed / warning / failed / success)
 *   when we have one — runs are newest-first, so `runs[0]` is latest
 * - Fallback to the workflow's stored `last_run_status` if no recent
 *   runs are loaded
 * - "idle" if the job has never run
 */
function leadingState(job: Job, runs: AnyRun[] | undefined): JobState {
  if (runs && isRunning(runs)) return "running";
  if (!job.enabled) return "idle";
  if (runs && runs.length > 0) return apiStatusToJobState(runs[0].status);
  if (job.last_run_status) return apiStatusToJobState(job.last_run_status);
  return "idle";
}

/**
 * Format a USD cost for the table's numeric column:
 * - `—` when there's no data or the value is zero (visually quieter)
 * - `$X` when the value is a whole dollar amount
 * - `$X.XX` otherwise
 */
function formatTableCost(usd: number | null | undefined): string {
  if (usd == null || usd === 0) return "—";
  if (Number.isInteger(usd)) return `$${usd}`;
  return `$${usd.toFixed(2)}`;
}

/**
 * Grid template shared by the row and (via JobsList) the column header.
 *
 * Below `md` the two cost columns are removed entirely (7 columns):
 *   [star] [state-dot] [name] [recent-dots] [schedule] [last] [next]
 *
 * At `md` and above we splice in the cost columns (9 columns):
 *   [star] [state-dot] [name] [recent-dots] [30d cost] [30d runs] [schedule] [last] [next]
 *
 * The corresponding cost cells use `hidden md:flex` so they only render
 * (and so only count toward the grid's children) once the cost columns
 * exist. CSS grid distributes children into the declared template in
 * source order, so removing the cells below `md` and changing the
 * template in lockstep keeps the columns aligned.
 */
export const JOBS_LIST_GRID =
  "grid grid-cols-[24px_20px_minmax(0,1.6fr)_120px_minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)] md:grid-cols-[24px_20px_minmax(0,1.6fr)_120px_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)] items-center gap-4";

export function JobsListRow({ job, runs, costSummary }: JobsListRowProps) {
  const leading = leadingState(job, runs);
  // API returns newest first; reverse so the oldest is on the left and the
  // most recent run is on the right.
  const recent = runs ? runs.slice(0, 7).reverse() : [];
  const running = leading === "running";
  const lastRun = running ? "Running now" : job.last_run_at ? formatTimeAgo(job.last_run_at) : "—";
  const nextRun = job.next_run_at ? formatTimeUntil(job.next_run_at) : "—";

  const cost30 = formatTableCost(costSummary?.last_30_days_total_usd);
  const runs30 =
    costSummary?.last_30_days_runs && costSummary.last_30_days_runs > 0
      ? costSummary.last_30_days_runs.toLocaleString()
      : "—";

  const [hovered, setHovered] = useState<{
    run: AnyRun;
    pos: { left: number; top: number };
  } | null>(null);

  function handleEnter(run: AnyRun) {
    return (e: React.MouseEvent<HTMLSpanElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      setHovered({
        run,
        pos: {
          left: rect.left + rect.width / 2,
          top: rect.bottom + 8,
        },
      });
    };
  }

  return (
    <>
      <Link
        href={`/workflows/${job.id}`}
        className={`group ${JOBS_LIST_GRID} px-4 py-3 rounded-input border border-border bg-surface hover:bg-surface-hover hover:border-border-strong transition-colors text-sm outline-none focus-visible:ring-2 focus-visible:ring-brand-ring`}
      >
        {/* Leading favorite star. Lives in its own column at the very
            edge of the row. The FavoriteToggle button already stops
            propagation on its underlying click, so this never triggers
            the row's Link navigation. */}
        <span
          className={[
            "shrink-0 transition-opacity flex items-center justify-center",
            job.is_favorited
              ? "opacity-100"
              : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
          ].join(" ")}
          // The Link is the row's navigation target; intercept any stray
          // clicks here as a defense-in-depth so the toggle never
          // navigates even if a future change to FavoriteToggle drops
          // its own preventDefault/stopPropagation. We MUST call
          // preventDefault as well as stopPropagation: stopPropagation
          // only prevents bubbling, but the browser still fires the
          // ancestor <a>'s default navigation for any click that
          // originates inside it. preventDefault cancels that.
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
          onMouseDown={(e) => {
            e.preventDefault();
            e.stopPropagation();
          }}
        >
          <FavoriteToggle jobId={job.id} favorited={job.is_favorited} size={14} />
        </span>
        <JobStateIndicator state={leading} variant="dot" size="sm" />
        <span className="font-medium text-fg truncate min-w-0">{job.name}</span>
        <span className="flex items-center gap-1">
          {recent.length === 0 ? (
            <span className="text-[10px] text-fg-faint italic">no runs</span>
          ) : (
            recent.map((run) => {
              const state = apiStatusToJobState(run.status);
              return (
                <span
                  key={run.run_id}
                  onMouseEnter={handleEnter(run)}
                  onMouseLeave={() => setHovered(null)}
                  className="inline-flex"
                >
                  <JobStateIndicator
                    state={state}
                    variant="dot"
                    size="sm"
                    className="hover:ring-2 hover:ring-fg-faint hover:scale-125 transition-all"
                  />
                </span>
              );
            })
          )}
        </span>
        {/* Cost columns — right-aligned, mono. Hidden below md to keep
            the row legible on narrow widths (e.g., Electron compact
            mode). */}
        <span className="hidden md:flex justify-end font-mono text-xs text-fg-muted tabular-nums">
          {cost30}
        </span>
        <span className="hidden md:flex justify-end font-mono text-xs text-fg-muted tabular-nums">
          {runs30}
        </span>
        <span className="font-mono text-xs text-fg-muted truncate">{job.schedule}</span>
        <span className="text-xs text-fg-muted truncate">{lastRun}</span>
        <span className="text-xs text-fg-muted truncate">{nextRun}</span>
      </Link>

      {hovered && (
        <RunTooltip
          run={hovered.run}
          jobName={job.name}
          left={hovered.pos.left}
          top={hovered.pos.top}
          transform="translateX(-50%)"
        />
      )}
    </>
  );
}
