"use client";

/**
 * HeroYearCostSlide
 *
 * Hero-sized slide showing the system-wide total cost for the last
 * year plus the total run count. Sibling of `HeroTotalCostSlide` —
 * same visual weight, different window. Uses the "ember" pastel
 * persona to feel warmer / more "annual report" than the 30-day view.
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
    <div className="gradient-ember-linear w-full h-full rounded-card flex flex-col items-center justify-center px-8 text-center text-[color:var(--color-ink-900)]">
      <div className="text-eyebrow !text-[color:var(--color-fg-tertiary)] inline-flex items-center gap-2 mb-4">
        <CalendarRange size={12} />
        <span>Total spend &middot; last year</span>
      </div>
      {loading ? (
        <div className="h-20 w-56 rounded bg-black/5 animate-pulse" />
      ) : (
        <div className="text-display text-[88px] md:text-[104px] num text-[color:var(--color-ink-950)]">
          {formatUsd(cost)}
        </div>
      )}
      <div className="mt-5 text-sm text-[color:var(--color-fg-tertiary)]">
        {loading ? (
          <span className="inline-block h-3 w-32 rounded bg-black/5 animate-pulse" />
        ) : (
          <>
            <span className="num font-semibold text-[color:var(--color-ink-900)]">
              {runs.toLocaleString()}
            </span>{" "}
            runs completed
          </>
        )}
      </div>
    </div>
  );
}
