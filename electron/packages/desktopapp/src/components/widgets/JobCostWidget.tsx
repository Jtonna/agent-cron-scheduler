"use client";

import { DollarSign } from "lucide-react";
import { NoirCallout } from "@/components/widgets/NoirCallout";
import type { WorkflowCostEntry } from "@/apis/types";

/**
 * JobCostWidget
 *
 * Per-workflow spend tile shown on `/workflows/[id]`. "Ember" mesh
 * persona with a noir-card hosting the 30-day total. Matches the
 * dashboard CostWidget composition so the visual energy carries
 * over to the workflow detail page.
 */

interface JobCostWidgetProps {
  summary: WorkflowCostEntry | null;
  loading: boolean;
}

function formatUsd(n: number): string {
  return `$${n.toFixed(2)}`;
}

function formatUsdPrecise(n: number): string {
  return `$${n.toFixed(3)}`;
}

export function JobCostWidget({ summary, loading }: JobCostWidgetProps) {
  const cs = summary?.cost_summary;
  const totalRuns = cs?.last_30_days_runs ?? 0;
  const totalUsd = cs?.last_30_days_total_usd ?? 0;
  const avgPerRun = totalRuns > 0 ? totalUsd / totalRuns : 0;
  const isEmpty = loading || !summary;

  return (
    <div
      data-mesh="ember"
      className="rounded-card p-6 min-h-[260px] flex flex-col text-[color:var(--color-ink-900)] border border-border-subtle"
    >
      <div className="text-eyebrow !text-fg-tertiary inline-flex items-center gap-2 mb-3">
        <DollarSign size={12} />
        <span>Cost &middot; last 30 days</span>
      </div>

      <NoirCallout eyebrow="Total spend">
        <div className="text-display text-4xl md:text-5xl num leading-none">
          {isEmpty ? "—" : formatUsd(totalUsd)}
        </div>
      </NoirCallout>

      <div className="mt-auto pt-5 grid grid-cols-2 gap-3">
        <Stat label="Avg / run" value={isEmpty ? "—" : formatUsdPrecise(avgPerRun)} />
        <Stat label="Runs" value={isEmpty ? "—" : String(totalRuns)} />
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-eyebrow !text-[9px] !text-fg-tertiary">{label}</div>
      <div className="mt-1 text-lg font-semibold num text-[color:var(--color-ink-900)]">
        {value}
      </div>
    </div>
  );
}
