"use client";

/**
 * HeroWorkflowInventorySlide
 *
 * Hero-sized slide showing how many workflows the daemon knows about
 * and the enabled / disabled / favorited split. Computes the split
 * from the same `useJobs()` data the rest of the dashboard already
 * reads — no new network call.
 *
 * Visual language: the cool "fog" pastel persona — calmer than the
 * money-themed slides, fits an inventory readout. Stats below the
 * mega numeral mirror the eyebrow/num pairing used by the rest of the
 * carousel.
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
    <div className="gradient-fog-linear w-full h-full rounded-card flex flex-col items-center justify-center px-8 text-center text-[color:var(--color-ink-900)]">
      <div className="text-eyebrow !text-[color:var(--color-fg-tertiary)] inline-flex items-center gap-2 mb-4">
        <Layers size={12} />
        <span>Workflow inventory</span>
      </div>
      {loading ? (
        <div className="h-20 w-40 rounded bg-black/5 animate-pulse" />
      ) : (
        <div className="text-display text-[88px] md:text-[104px] num text-[color:var(--color-ink-950)]">
          {total.toLocaleString()}
        </div>
      )}
      <div className="mt-6 flex items-center gap-8">
        <Stat label="Enabled" value={enabled} />
        <span className="h-8 w-px bg-[color:var(--color-border-strong)] opacity-40" aria-hidden />
        <Stat label="Disabled" value={disabled} />
        <span className="h-8 w-px bg-[color:var(--color-border-strong)] opacity-40" aria-hidden />
        <Stat label="Favorited" value={favorited} />
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex flex-col items-center gap-1">
      <span className="num font-semibold text-lg text-[color:var(--color-ink-900)]">
        {value}
      </span>
      <span className="text-eyebrow !text-[10px] !text-[color:var(--color-fg-tertiary)]">
        {label}
      </span>
    </div>
  );
}
