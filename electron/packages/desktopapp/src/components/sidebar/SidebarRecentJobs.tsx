"use client";

import { JobSidebarItem } from "./JobSidebarItem";
import { useJobRuns } from "@/apis/useJobRuns";
import type { Job } from "@/apis/types";

/**
 * SidebarRecentJobs
 *
 * "Recent" sidebar section content — sorts jobs by `last_run_at` desc
 * (falling back to `created_at`), takes the top N, and renders one
 * JobSidebarItem per job. Renders a graceful empty state if there are
 * no jobs.
 */

interface SidebarRecentJobsProps {
  jobs: Job[];
  /** Maximum number of jobs to show. Defaults to 7. */
  max?: number;
}

/**
 * Wrapper component used so that we can call the useJobRuns hook once per
 * rendered job (hooks can't be called inside loops conditionally — but a
 * dedicated component per row keeps the rules-of-hooks contract clean).
 */
function SidebarItemWithRuns({ job }: { job: Job }) {
  const { runs } = useJobRuns(job.id, 7);
  return <JobSidebarItem job={job} runs={runs} />;
}

export function SidebarRecentJobs({ jobs, max = 7 }: SidebarRecentJobsProps) {
  // Sort + slice over a small array (typically <50 jobs) — useMemo isn't worth it.
  const sorted = [...jobs].sort((a, b) => {
    const aT = a.last_run_at ? new Date(a.last_run_at).getTime() : new Date(a.created_at).getTime();
    const bT = b.last_run_at ? new Date(b.last_run_at).getTime() : new Date(b.created_at).getTime();
    return bT - aT;
  });
  const recent = sorted.slice(0, max);

  if (recent.length === 0) {
    return <div className="px-2 py-2 text-xs text-fg-subtle italic">No jobs yet</div>;
  }

  return (
    <div className="flex flex-col gap-0.5">
      {recent.map((job) => (
        <SidebarItemWithRuns key={job.id} job={job} />
      ))}
    </div>
  );
}
