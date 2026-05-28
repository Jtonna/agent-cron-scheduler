"use client";

import { JobSidebarItem } from "./JobSidebarItem";
import { useJobRuns } from "@/apis/useJobRuns";
import type { Job } from "@/apis/types";

/**
 * SidebarFavoritedJobs
 *
 * "Favorited" sidebar section content — renders the supplied jobs
 * (already filtered by `is_favorited`) alphabetically as
 * JobSidebarItems, mirroring the Recent section's layout.
 */

interface SidebarFavoritedJobsProps {
  jobs: Job[];
}

/**
 * Wrapper component used so that we can call the useJobRuns hook once per
 * rendered job (hooks can't be called inside loops conditionally — but a
 * dedicated component per row keeps the rules-of-hooks contract clean).
 */
function FavoritedItemWithRuns({ job }: { job: Job }) {
  const { runs } = useJobRuns(job.id, 7);
  return <JobSidebarItem job={job} runs={runs} />;
}

export function SidebarFavoritedJobs({ jobs }: SidebarFavoritedJobsProps) {
  const sorted = [...jobs].sort((a, b) => a.name.localeCompare(b.name));

  return (
    <div className="flex flex-col gap-0.5">
      {sorted.map((job) => (
        <FavoritedItemWithRuns key={job.id} job={job} />
      ))}
    </div>
  );
}
