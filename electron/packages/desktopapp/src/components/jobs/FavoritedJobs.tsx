import { Star } from "lucide-react";
import { Pill } from "@/components/ui/Pill";

/**
 * FavoritedJobs
 *
 * Inline strip of favorited jobs rendered on the home hero. Each entry
 * is a Pill linking to the job detail page; the strip wraps to multiple
 * lines when the row overflows. Renders a faint empty-state row when
 * the `jobs` array is empty.
 */

export interface FavoritedJob {
  id: string;
  name: string;
}

interface FavoritedJobsProps {
  jobs?: FavoritedJob[];
}

export function FavoritedJobs({ jobs = [] }: FavoritedJobsProps) {
  if (jobs.length === 0) {
    return (
      <div className="flex items-center gap-2 text-sm text-fg-subtle">
        <Star size={14} />
        <span>No favorited workflows</span>
      </div>
    );
  }

  return (
    <div className="flex items-start gap-2">
      <Star size={14} className="text-fg-subtle shrink-0 mt-1.5" />
      <div className="flex flex-wrap items-center gap-1.5">
        {jobs.map((job) => (
          <Pill key={job.id} href={`/workflows/${job.id}`} label={job.name} size="sm" />
        ))}
      </div>
    </div>
  );
}
