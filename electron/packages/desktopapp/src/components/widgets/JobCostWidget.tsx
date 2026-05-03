"use client";

import { DollarSign } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { JobCostSummaryResponse } from "@/apis/types";

/**
 * JobCostWidget
 *
 * Per-job cost tile showing total cost, average cost per run, and total
 * runs over the summary's timeframe (typically 30 days). Renders a
 * skeleton row state while loading or when `summary` is null.
 */

interface JobCostWidgetProps {
  summary: JobCostSummaryResponse | null;
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
          <Row label="Total" value={formatUsd(summary.summary.total_cost_usd)} />
          <Row label="Avg / run" value={formatUsdPrecise(summary.summary.avg_cost_per_run)} />
          <Row label="Runs" value={String(summary.summary.total_runs)} />
        </div>
      )}
    </StatWidget>
  );
}
