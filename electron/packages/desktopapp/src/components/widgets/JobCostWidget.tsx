"use client";

import { DollarSign } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { WorkflowCostEntry } from "@/apis/types";

/**
 * JobCostWidget
 *
 * Per-workflow cost tile showing total cost, average cost per run, and
 * total runs over the last 30 days. Renders a skeleton row state while
 * loading or when `summary` is null.
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

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="text-fg-muted text-sm">{label}</span>
      <span className="font-mono text-sm text-fg">{value}</span>
    </div>
  );
}

function SkeletonRow() {
  return (
    <div className="flex items-center justify-between py-1">
      <span className="h-3 w-12 rounded bg-surface-tertiary inline-block" />
      <span className="h-3 w-14 rounded bg-surface-tertiary inline-block" />
    </div>
  );
}

export function JobCostWidget({ summary, loading }: JobCostWidgetProps) {
  const cs = summary?.cost_summary;
  const totalRuns = cs?.last_30_days_runs ?? 0;
  const totalUsd = cs?.last_30_days_total_usd ?? 0;
  const avgPerRun = totalRuns > 0 ? totalUsd / totalRuns : 0;

  return (
    <StatWidget title="Cost" icon={<DollarSign size={14} />}>
      {loading || !summary ? (
        <div className="flex flex-col gap-1.5 animate-pulse">
          <SkeletonRow />
          <SkeletonRow />
          <SkeletonRow />
        </div>
      ) : (
        <div className="flex flex-col gap-0.5">
          <Row label="Total" value={formatUsd(totalUsd)} />
          <Row label="Avg / run" value={formatUsdPrecise(avgPerRun)} />
          <Row label="Runs" value={String(totalRuns)} />
        </div>
      )}
    </StatWidget>
  );
}
