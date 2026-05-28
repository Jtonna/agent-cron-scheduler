"use client";

import Link from "next/link";
import { useState } from "react";
import { formatTimeAgo, formatTimeUntil } from "@/apis/format";
import type { Job } from "@/apis/types";
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
 * One row of the jobs table on `/jobs`. Renders a leading state dot,
 * the job name (with a hover-reveal favorite star to its right), up to 7
 * recent run dots (with hover tooltips), the cron schedule, last-run
 * time, and next-run time. The row itself is a navigation link to the
 * job detail page.
 *
 * Favorite affordance: when the job IS favorited the star is always
 * shown (it doubles as a "this is favorited" indicator); when it is not,
 * the star fades in only on row hover/focus so the row stays uncluttered
 * for the common case.
 */

interface JobsListRowProps {
  job: Job;
  /** Most recent runs for this job, newest first. Up to 7 are rendered. */
  runs?: AnyRun[];
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

export function JobsListRow({ job, runs }: JobsListRowProps) {
  const leading = leadingState(job, runs);
  // API returns newest first; reverse so the oldest is on the left and the
  // most recent run is on the right.
  const recent = runs ? runs.slice(0, 7).reverse() : [];
  const running = leading === "running";
  const lastRun = running ? "Running now" : job.last_run_at ? formatTimeAgo(job.last_run_at) : "—";
  const nextRun = job.next_run_at ? formatTimeUntil(job.next_run_at) : "—";

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
        href={`/jobs/${job.id}`}
        className="group grid grid-cols-[20px_minmax(0,1.6fr)_120px_minmax(0,1.4fr)_minmax(0,1fr)_minmax(0,1fr)] items-center gap-4 px-4 py-3 rounded-input border border-border bg-surface hover:bg-surface-hover hover:border-border-strong transition-colors text-sm outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
      >
        <JobStateIndicator state={leading} variant="dot" size="sm" />
        <span className="font-medium text-fg truncate inline-flex items-center gap-1.5 min-w-0">
          <span className="truncate">{job.name}</span>
          {/* Hover-reveal favorite: always shown when favorited (status
              indicator), fades in on row hover/focus when not. Less
              visually intrusive than a fixed star next to every name. */}
          <span
            className={[
              "shrink-0 transition-opacity",
              job.is_favorited
                ? "opacity-100"
                : "opacity-0 group-hover:opacity-100 group-focus-within:opacity-100",
            ].join(" ")}
          >
            <FavoriteToggle jobId={job.id} favorited={job.is_favorited} size={14} />
          </span>
        </span>
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
