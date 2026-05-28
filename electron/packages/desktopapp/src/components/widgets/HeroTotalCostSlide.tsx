"use client";

/**
 * HeroTotalCostSlide
 *
 * Hero-sized slide showing the system-wide total cost for the last 30
 * days plus the total run count over the same window. Used by
 * `SystemHeroCarousel` on the dashboard `/` route. Renders a large
 * money headline against the same hero gradient as the original
 * illustration placeholder so swapping between slides keeps the visual
 * weight of the page constant.
 */

import { DollarSign } from "lucide-react";
import type { WorkflowCostSummary } from "@/apis/types";

interface HeroTotalCostSlideProps {
  summary: WorkflowCostSummary | null;
  loading: boolean;
}

function formatUsd(n: number): string {
  if (n >= 1000) return `$${n.toFixed(0)}`;
  return `$${n.toFixed(2)}`;
}

export function HeroTotalCostSlide({ summary, loading }: HeroTotalCostSlideProps) {
  const hasData = !loading && summary !== null;
  const cost = hasData ? summary.last_30_days_total_usd : 0;
  const runs = hasData ? summary.last_30_days_runs : 0;

  return (
    <div className="w-full h-full bg-gradient-to-br from-gradient-hero-from via-gradient-hero-via to-gradient-hero-to rounded-card flex flex-col items-center justify-center px-8 text-center">
      <div className="flex items-center gap-2 text-fg-muted text-xs uppercase tracking-wider mb-3">
        <DollarSign size={14} />
        <span>Total spend · last 30 days</span>
      </div>
      {loading ? (
        <div className="h-20 w-56 rounded bg-surface-tertiary/50 animate-pulse" />
      ) : (
        <div className="text-[68px] font-extrabold text-fg leading-none tracking-tight">
          {formatUsd(cost)}
        </div>
      )}
      <div className="mt-4 text-fg-muted text-sm">
        {loading ? (
          <span className="inline-block h-3 w-32 rounded bg-surface-tertiary/50 animate-pulse" />
        ) : (
          <>
            <span className="font-mono text-fg">{runs.toLocaleString()}</span> runs completed
          </>
        )}
      </div>
    </div>
  );
}
