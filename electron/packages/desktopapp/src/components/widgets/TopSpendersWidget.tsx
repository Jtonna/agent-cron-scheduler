"use client";

import Link from "next/link";
import { TrendingUp } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { GlobalTopJob } from "@/apis/types";

/**
 * TopSpendersWidget
 *
 * Dashboard tile listing the top 5 jobs by cumulative cost. Each row
 * links to the job detail page. Renders a friendly empty state when
 * there is no cost data yet.
 */

interface TopSpendersWidgetProps {
  jobs: GlobalTopJob[];
}

function formatUsd(n: number): string {
  return `$${n.toFixed(2)}`;
}

export function TopSpendersWidget({ jobs }: TopSpendersWidgetProps) {
  const top = jobs.slice(0, 5);

  return (
    <StatWidget title="Top spenders" icon={<TrendingUp size={14} />}>
      {top.length === 0 ? (
        <div className="text-fg-subtle text-sm py-2">No cost data yet</div>
      ) : (
        <ol className="flex flex-col gap-2">
          {top.map((job, idx) => (
            <li key={job.job_id} className="flex items-center gap-2 text-sm">
              <span className="w-4 text-fg-subtle font-mono text-xs shrink-0">{idx + 1}</span>
              <Link
                href={`/jobs/${job.job_id}`}
                className="text-fg hover:text-brand truncate flex-1 min-w-0"
              >
                {job.job_name}
              </Link>
              <span className="font-mono text-xs text-fg-muted shrink-0">
                {formatUsd(job.total_cost)}
              </span>
              <span className="text-xs text-fg-subtle shrink-0 w-12 text-right">
                {job.total_runs} {job.total_runs === 1 ? "run" : "runs"}
              </span>
            </li>
          ))}
        </ol>
      )}
    </StatWidget>
  );
}
