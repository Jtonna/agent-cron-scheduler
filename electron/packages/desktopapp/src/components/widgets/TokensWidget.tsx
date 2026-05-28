"use client";

import { Type } from "lucide-react";
import { NoirCallout } from "@/components/widgets/NoirCallout";
import type { WorkflowCostSummary } from "@/apis/types";

/**
 * TokensWidget
 *
 * System-wide token usage tile. "Lemon" pastel mesh persona surface with
 * a noir-card callout for the headline in/out pair and a stat row for
 * the today + week totals. Matches CostWidget composition so the two
 * read as a pair.
 */

interface TokensWidgetProps {
  summary: WorkflowCostSummary | null;
  loading: boolean;
}

function formatTokens(n: number): string {
  if (n < 1000) return String(Math.trunc(n));
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

export function TokensWidget({ summary, loading }: TokensWidgetProps) {
  const isEmpty = loading || !summary;
  const t = summary
    ? computeTotals(summary)
    : {
        todayInput: 0,
        todayOutput: 0,
        weekInput: 0,
        weekOutput: 0,
        monthInput: 0,
        monthOutput: 0,
      };

  return (
    <div
      data-mesh="lemon"
      className="rounded-card p-6 min-h-[260px] flex flex-col text-[color:var(--color-ink-900)] border border-border-subtle"
    >
      <div className="text-eyebrow !text-fg-tertiary inline-flex items-center gap-2 mb-3">
        <Type size={12} />
        <span>Tokens &middot; this month</span>
      </div>

      <NoirCallout eyebrow="In / out">
        <div className="text-display text-3xl md:text-4xl num leading-none">
          {isEmpty ? "—" : formatPair(t.monthInput, t.monthOutput)}
        </div>
      </NoirCallout>

      <div className="mt-auto pt-5 grid grid-cols-2 gap-3">
        <Stat
          label="Today"
          value={isEmpty ? "—" : formatPair(t.todayInput, t.todayOutput)}
        />
        <Stat
          label="This week"
          value={isEmpty ? "—" : formatPair(t.weekInput, t.weekOutput)}
        />
      </div>
    </div>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div className="text-eyebrow !text-[9px] !text-fg-tertiary">{label}</div>
      <div className="mt-1 text-sm font-semibold num text-[color:var(--color-ink-900)]">
        {value}
      </div>
    </div>
  );
}
