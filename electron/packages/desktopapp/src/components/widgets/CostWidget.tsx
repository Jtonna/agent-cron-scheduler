"use client";

import { DollarSign } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { GlobalCostSummaryResponse } from "@/apis/types";

/**
 * CostWidget
 *
 * Dashboard tile showing today/week/month USD totals. Renders a skeleton
 * row state while loading or when `summary` is null.
 */

interface CostWidgetProps {
  summary: GlobalCostSummaryResponse | null;
  loading: boolean;
}

function formatUsd(n: number): string {
  return `$${n.toFixed(2)}`;
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

export function CostWidget({ summary, loading }: CostWidgetProps) {
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
          <Row label="Today" value={formatUsd(summary.today_usd)} />
          <Row label="This week" value={formatUsd(summary.week_usd)} />
          <Row label="This month" value={formatUsd(summary.month_usd)} />
        </div>
      )}
    </StatWidget>
  );
}
