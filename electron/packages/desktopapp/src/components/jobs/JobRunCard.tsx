"use client";

import { useState } from "react";
import { Button } from "react-aria-components";
import { Clock, DollarSign, ArrowRight } from "lucide-react";
import { JobStateIndicator, type JobState } from "@/components/ui/JobStateIndicator";

/**
 * JobRunCard
 *
 * Card representation of a single job run, used on the home dashboard's
 * "Recent runs" grid. Hovering slides the inner content left to reveal
 * a brand-colored arrow affordance on the right edge.
 */

/** Re-exported so existing consumers (e.g. `import { JobStatus } from ...`) keep working. */
export type JobStatus = JobState;

export interface JobRun {
  name: string;
  status: JobStatus;
  duration: string;
  timeAgo: string;
  cost?: string;
}

interface JobRunCardProps {
  job: JobRun;
  onClick?: (job: JobRun) => void;
}

export function JobRunCard({ job, onClick }: JobRunCardProps) {
  const [hovered, setHovered] = useState(false);

  return (
    <Button
      onPress={() => onClick?.(job)}
      onHoverStart={() => setHovered(true)}
      onHoverEnd={() => setHovered(false)}
      className="relative overflow-hidden border border-border rounded-card bg-surface cursor-pointer hover:border-brand-ring transition-colors text-left outline-none focus-visible:ring-2 focus-visible:ring-brand-ring w-full"
      aria-label={`${job.name} — ${job.status}`}
    >
      <div
        className={`p-4 transition-transform duration-200 ease-out ${hovered ? "-translate-x-12" : "translate-x-0"}`}
      >
        {/* Top row — name + cost */}
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <JobStateIndicator state={job.status} variant="dot" size="sm" />
            <span className="text-sm font-semibold text-fg truncate">{job.name}</span>
          </div>
          {job.cost && (
            <span className="inline-flex items-center gap-1 text-xs text-fg-subtle font-mono">
              <DollarSign size={11} />
              {job.cost}
            </span>
          )}
        </div>
        {/* Bottom row — duration, time, status badge */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3 text-xs text-fg-subtle">
            <span className="inline-flex items-center gap-1">
              <Clock size={12} />
              {job.duration}
            </span>
            <span>{job.timeAgo}</span>
          </div>
          <JobStateIndicator state={job.status} variant="badge" />
        </div>
      </div>

      {/* Slide-in panel from right */}
      <div
        className={`absolute inset-y-0 right-0 w-12 flex items-center justify-center bg-brand text-surface transition-transform duration-200 ease-out ${
          hovered ? "translate-x-0" : "translate-x-full"
        }`}
        aria-hidden
      >
        <ArrowRight size={16} />
      </div>
    </Button>
  );
}
