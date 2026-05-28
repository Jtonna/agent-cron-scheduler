"use client";

import { Type } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { WorkflowCostSummary } from "@/apis/types";

interface TokensWidgetProps {
  summary: WorkflowCostSummary | null;
  loading: boolean;
}

function formatTokens(n: number): string {
  if (n < 1000) {
    return String(Math.trunc(n));
  }
  if (n < 1_000_000) {
    const s = (n / 1000).toFixed(1);
    return (s.endsWith(".0") ? s.slice(0, -2) : s) + "k";
  }
  const s = (n / 1_000_000).toFixed(1);
  return (s.endsWith(".0") ? s.slice(0, -2) : s) + "M";
}

function formatPair(input: number, output: number): string {
  return `${formatTokens(input)} / ${formatTokens(output)}`;
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
  todayInput: number;
  todayOutput: number;
  weekInput: number;
  weekOutput: number;
  monthInput: number;
  monthOutput: number;
} {
  const today = todayKey();
  const weekKeys = lastNDateKeys(7);

  let todayInput = 0;
  let todayOutput = 0;
  let weekInput = 0;
  let weekOutput = 0;
  for (const bucket of summary.daily_buckets) {
    if (bucket.date === today) {
      todayInput += bucket.total_input_tokens;
      todayOutput += bucket.total_output_tokens;
    }
    if (weekKeys.has(bucket.date)) {
      weekInput += bucket.total_input_tokens;
      weekOutput += bucket.total_output_tokens;
    }
  }

  return {
    todayInput,
    todayOutput,
    weekInput,
    weekOutput,
    monthInput: summary.last_30_days_input_tokens,
    monthOutput: summary.last_30_days_output_tokens,
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

export function TokensWidget({ summary, loading }: TokensWidgetProps) {
  return (
    <StatWidget title="Tokens" icon={<Type size={14} />}>
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
              <div className="text-display text-2xl num text-fg leading-none mb-1">
                {formatPair(totals.monthInput, totals.monthOutput)}
              </div>
              <div className="text-eyebrow !text-fg-subtle mb-3">This month &middot; in / out</div>
              <div className="h-px bg-border-subtle mb-2" />
              <div className="flex flex-col gap-0.5">
                <Row label="Today" value={formatPair(totals.todayInput, totals.todayOutput)} />
                <Row label="This week" value={formatPair(totals.weekInput, totals.weekOutput)} />
              </div>
            </div>
          );
        })()
      )}
    </StatWidget>
  );
}
