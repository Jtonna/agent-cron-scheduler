"use client";

import { FavoriteToggle } from "./FavoriteToggle";
import {
  JobStateIndicator,
  type JobState,
} from "@/components/ui/JobStateIndicator";
import type { Job } from "@/apis/types";

/**
 * WorkflowHeader
 *
 * Identity block atop the workflow detail page (`/workflows/[id]`). Mirrors
 * the visual idiom of `RunHeader` (used on the run-detail page): a big
 * workflow name as h1 with a meta line beneath carrying cron + timezone +
 * enabled-status pill. The favorite star sits to the left of the name and
 * toggles via `useFavorite()` (through `FavoriteToggle`).
 *
 * The same favorite control is also surfaced in the STATUS section of
 * `JobDetailSidebar`, so users can favorite/unfavorite from either the
 * main page or the sidebar for consistency.
 */

export interface WorkflowHeaderProps {
  job: Job;
}

export function WorkflowHeader({ job }: WorkflowHeaderProps) {
  const state: JobState = job.enabled ? "success" : "idle";
  const stateLabel = job.enabled ? "Enabled" : "Disabled";

  return (
    <div className="flex flex-col gap-1 min-w-0">
      <div className="flex items-center gap-3 min-w-0">
        <div className="shrink-0">
          <FavoriteToggle
            jobId={job.id}
            favorited={job.is_favorited}
            size={20}
          />
        </div>
        <h1
          className="text-2xl font-extrabold tracking-tight text-fg truncate"
          title={job.name}
        >
          {job.name}
        </h1>
      </div>
      <div className="flex items-center gap-2 min-w-0 flex-wrap">
        <span
          className="font-mono text-xs text-fg-muted truncate"
          title={job.schedule}
        >
          {job.schedule}
        </span>
        <span className="text-fg-faint">·</span>
        <span
          className="text-xs text-fg-muted truncate"
          title={job.timezone}
        >
          {job.timezone}
        </span>
        <span className="text-fg-faint">·</span>
        <JobStateIndicator
          state={state}
          variant="label"
          label={stateLabel}
        />
      </div>
    </div>
  );
}
