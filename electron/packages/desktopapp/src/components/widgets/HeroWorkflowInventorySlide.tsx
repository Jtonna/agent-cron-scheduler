"use client";

/**
 * HeroWorkflowInventorySlide
 *
 * Hero-sized slide showing how many workflows the daemon knows about
 * and the enabled / disabled split. Computes the split from the same
 * `useJobs()` data the rest of the dashboard already reads — no new
 * network call.
 */

import { Layers } from "lucide-react";
import type { Job } from "@/apis/types";

interface HeroWorkflowInventorySlideProps {
  jobs: Job[];
  loading?: boolean;
}

export function HeroWorkflowInventorySlide({
  jobs,
  loading = false,
}: HeroWorkflowInventorySlideProps) {
  const total = jobs.length;
  const enabled = jobs.filter((j) => j.enabled).length;
  const disabled = total - enabled;
  const favorited = jobs.filter((j) => j.is_favorited).length;

  return (
    <div className="w-full h-full bg-gradient-to-br from-gradient-hero-from via-gradient-hero-via to-gradient-hero-to rounded-card flex flex-col items-center justify-center px-8 text-center">
      <div className="flex items-center gap-2 text-fg-muted text-xs uppercase tracking-wider mb-3">
        <Layers size={14} />
        <span>Workflow inventory</span>
      </div>
      {loading ? (
        <div className="h-20 w-40 rounded bg-surface-tertiary/50 animate-pulse" />
      ) : (
        <div className="text-[68px] font-extrabold text-fg leading-none tracking-tight">
          {total.toLocaleString()}
        </div>
      )}
      <div className="mt-5 flex items-center gap-6 text-sm">
        <div className="flex flex-col items-center">
          <span className="font-mono text-fg text-base">{enabled}</span>
          <span className="text-fg-muted text-xs uppercase tracking-wider">Enabled</span>
        </div>
        <span className="text-fg-faint">·</span>
        <div className="flex flex-col items-center">
          <span className="font-mono text-fg text-base">{disabled}</span>
          <span className="text-fg-muted text-xs uppercase tracking-wider">Disabled</span>
        </div>
        <span className="text-fg-faint">·</span>
        <div className="flex flex-col items-center">
          <span className="font-mono text-fg text-base">{favorited}</span>
          <span className="text-fg-muted text-xs uppercase tracking-wider">Favorited</span>
        </div>
      </div>
    </div>
  );
}
