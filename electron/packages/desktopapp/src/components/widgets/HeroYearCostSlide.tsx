"use client";

/**
 * HeroYearCostSlide
 *
 * Hero-sized slide showing the system-wide total cost for the last
 * year plus the total run count. Sibling of `HeroTotalCostSlide` —
 * same visual weight, different window.
 */

import { CalendarRange } from "lucide-react";
import type { WorkflowCostSummary } from "@/apis/types";

interface HeroYearCostSlideProps {
  summary: WorkflowCostSummary | null;
  loading: boolean;
}

function formatUsd(n: number): string {
  if (n >= 1000) return `$${n.toFixed(0)}`;
  return `$${n.toFixed(2)}`;
}

export function HeroYearCostSlide({ summary, loading }: HeroYearCostSlideProps) {
  const hasData = !loading && summary !== null;
  const cost = hasData ? summary.last_year_total_usd : 0;
  const runs = hasData ? summary.last_year_runs : 0;

  return (
    <div className="w-full h-full bg-gradient-to-br from-gradient-hero-from via-gradient-hero-via to-gradient-hero-to rounded-card flex flex-col items-center justify-center px-8 text-center">
      <div className="flex items-center gap-2 text-fg-muted text-xs uppercase tracking-wider mb-3">
        <CalendarRange size={14} />
        <span>Total spend · last year</span>
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
