"use client";

import { DollarSign } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { WorkflowCostSummary } from "@/apis/types";

interface CostWidgetProps {
  summary: WorkflowCostSummary | null;
  loading: boolean;
}

function formatUsd(n: number): string {
  return `$${n.toFixed(2)}`;
}

function todayKey(): string {
  return new Date().toISOString().slice(0, 10);
}

function lastNDateKeys(n: number): Set<string> {
  const keys = new Set<string>();
  const today = new Date();
  for (let i = 0; i < n; i += 1) {
    const d = new Date(today);
    d.setDate(today.getDate() - i);
    keys.add(d.toISOString().slice(0, 10));
  }
  return keys;
}

function computeTotals(summary: WorkflowCostSummary): {
  today: number;
  week: number;
  month: number;
} {
  const today = todayKey();
  const weekKeys = lastNDateKeys(7);

  let todayTotal = 0;
  let weekTotal = 0;
  for (const bucket of summary.daily_buckets) {
    if (bucket.date === today) todayTotal += bucket.total_usd;
    if (weekKeys.has(bucket.date)) weekTotal += bucket.total_usd;
  }

  return {
    today: todayTotal,
    week: weekTotal,
    month: summary.last_30_days_total_usd,
  };
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
        (() => {
          const totals = computeTotals(summary);
          return (
            <div className="flex flex-col">
              <div className="text-display text-3xl num text-fg leading-none mb-3">
                {formatUsd(totals.month)}
              </div>
              <div className="text-eyebrow !text-fg-subtle mb-2">This month</div>
              <div className="h-px bg-border-subtle mb-2" />
              <div className="flex flex-col gap-0.5">
                <Row label="Today" value={formatUsd(totals.today)} />
                <Row label="This week" value={formatUsd(totals.week)} />
              </div>
            </div>
          );
        })()
      )}
    </StatWidget>
  );
}
