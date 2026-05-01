"use client";

import NextLink from "next/link";
import { useRef, useState } from "react";
import { Link as AriaLink } from "react-aria-components";
import { CornerDownRight } from "lucide-react";
import type { Job, JobRun } from "@/apis/types";
import { formatTimeAgo } from "@/apis/format";
import { JobStateIndicator, apiStatusToJobState } from "@/components/ui/JobStateIndicator";
import { RunTooltip } from "@/components/ui/RunTooltip";

/**
 * JobSidebarItem
 *
 * Sidebar entry for a single job — name link plus a row of recent run
 * dots and a trailing "time ago" label. Each dot is itself a link to
 * the run detail page; hovering reveals a portal-anchored RunTooltip
 * positioned to the right of the sidebar.
 */

interface JobSidebarItemProps {
  job: Job;
  runs: JobRun[];
  /** How many run dots to show. Defaults to 7. */
  maxRuns?: number;
}

function shortRunId(runId: string): string {
  return runId.slice(0, 8);
}

export function JobSidebarItem({ job, runs, maxRuns = 7 }: JobSidebarItemProps) {
  // The API returns newest-first. The trailing time-ago label always
  // tracks runs[0] (the latest), but the visual dot order is reversed
  // so old is on the left and the most recent run is on the right.
  const latest = runs[0];
  const recent = runs.slice(0, maxRuns).reverse();

  const rowRef = useRef<HTMLDivElement>(null);
  const [hoveredRun, setHoveredRun] = useState<JobRun | null>(null);
  const [anchorTop, setAnchorTop] = useState(0);

  function handleEnter(run: JobRun) {
    if (rowRef.current) {
      const rect = rowRef.current.getBoundingClientRect();
      setAnchorTop(rect.top + rect.height / 2);
    }
    setHoveredRun(run);
  }

  function handleLeave() {
    setHoveredRun(null);
  }

  return (
    <>
      <div className="flex flex-col gap-0.5 py-1.5 px-2 rounded hover:bg-surface-hover transition-colors">
        <NextLink
          href={`/jobs/${job.id}`}
          className="text-sm text-fg-secondary hover:text-fg truncate"
        >
          {job.name}
        </NextLink>

        <div ref={rowRef} className="flex items-center gap-2 pl-1">
          <CornerDownRight size={12} className="text-fg-faint shrink-0" aria-hidden />
          {recent.length === 0 ? (
            <span className="text-[11px] text-fg-faint italic">No runs yet</span>
          ) : (
            <>
              <div className="flex items-center gap-1">
                {recent.map((run) => {
                  const state = apiStatusToJobState(run.status);
                  return (
                    <AriaLink
                      key={run.run_id}
                      href={`/jobs/${job.id}/runs/${run.run_id}`}
                      aria-label={`Run ${shortRunId(run.run_id)}`}
                      onHoverStart={() => handleEnter(run)}
                      onHoverEnd={handleLeave}
                      onFocus={() => handleEnter(run)}
                      onBlur={handleLeave}
                      className="block outline-none rounded-full focus-visible:ring-2 focus-visible:ring-brand-ring"
                    >
                      <JobStateIndicator
                        state={state}
                        variant="dot"
                        size="md"
                        className="hover:ring-2 hover:ring-fg-faint hover:scale-110 transition-all"
                      />
                    </AriaLink>
                  );
                })}
              </div>
              {latest && (
                <span className="ml-auto text-[10px] text-fg-faint shrink-0">
                  {formatTimeAgo(latest.started_at)}
                </span>
              )}
            </>
          )}
        </div>
      </div>

      {hoveredRun && (
        <RunTooltip
          run={hoveredRun}
          jobName={job.name}
          left="calc(var(--width-sidebar) + 12px)"
          top={anchorTop}
          transform="translateY(-50%)"
        />
      )}
    </>
  );
}
