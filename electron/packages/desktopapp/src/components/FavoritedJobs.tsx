import { Star } from "lucide-react";
import Link from "next/link";

interface FavoritedJobsProps {
  jobs?: string[];
}

export function FavoritedJobs({ jobs = [] }: FavoritedJobsProps) {
  if (jobs.length === 0) {
    return (
      <div className="flex items-center gap-2 text-sm text-gray-400">
        <Star size={14} />
        <span>No favorited jobs</span>
      </div>
    );
  }

  return (
    <div className="flex items-center gap-2">
      <Star size={14} className="text-gray-400 shrink-0" />
      <div className="flex items-center gap-1.5 overflow-x-auto">
        {jobs.map((job) => (
          <Link
            key={job}
            href={`/jobs/${job}`}
            className="text-xs text-gray-500 hover:text-gray-900 hover:bg-gray-100 px-2.5 py-1 rounded-lg transition-colors whitespace-nowrap"
          >
            {job}
          </Link>
        ))}
      </div>
    </div>
  );
}
