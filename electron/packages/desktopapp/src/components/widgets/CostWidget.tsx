"use client";

import { DollarSign } from "lucide-react";
import type { WorkflowCostSummary } from "@/apis/types";

/**
 * CostWidget
 *
 * System-wide spend tile. Pastel "peach" mesh persona surface hosting
 * the headline 30-day total directly on the surface (no noir callout —
 * per user feedback the dramatic black contrast was excessive for the
 * top-level stat tiles), with a stat row underneath for today + this
 * week.
 */

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

export function CostWidget({ summary, loading }: CostWidgetProps) {
  const totals = summary ? computeTotals(summary) : { today: 0, week: 0, month: 0 };
  const isEmpty = loading || !summary;

  return (
    <div
      data-mesh="peach"
      className="rounded-card p-6 min-h-[260px] flex flex-col text-[color:var(--color-ink-900)] border border-border-subtle"
    >
      <div className="text-eyebrow !text-fg-tertiary inline-flex items-center gap-2 mb-3">
        <DollarSign size={12} />
        <span>Cost &middot; this month</span>
      </div>

      <div>
        <div className="text-eyebrow !text-fg-tertiary mb-2">Total spend</div>
        <div className="text-display text-4xl md:text-5xl num leading-none text-[color:var(--color-ink-900)]">
          {isEmpty ? "—" : formatUsd(totals.month)}
        </div>
      </div>

      <div className="mt-auto pt-5 grid grid-cols-2 gap-3">
        <Stat label="Today" value={isEmpty ? "—" : formatUsd(totals.today)} />
        <Stat label="This week" value={isEmpty ? "—" : formatUsd(totals.week)} />
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
