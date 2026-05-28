"use client";

/**
 * HeroTokensSlide
 *
 * Hero-sized slide showing the system-wide input → output token flow
 * for the last 30 days. Uses the `→` arrow notation from `RunStats` so
 * the dashboard speaks the same dialect as the run detail view.
 */

import { Type } from "lucide-react";
import type { WorkflowCostSummary } from "@/apis/types";

interface HeroTokensSlideProps {
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

export function HeroTokensSlide({ summary, loading }: HeroTokensSlideProps) {
  const hasData = !loading && summary !== null;
  const input = hasData ? summary.last_30_days_input_tokens : 0;
  const output = hasData ? summary.last_30_days_output_tokens : 0;

  return (
    <div className="w-full h-full bg-gradient-to-br from-gradient-hero-from via-gradient-hero-via to-gradient-hero-to rounded-card flex flex-col items-center justify-center px-8 text-center">
      <div className="flex items-center gap-2 text-fg-muted text-xs uppercase tracking-wider mb-4">
        <Type size={14} />
        <span>Token flow · last 30 days</span>
      </div>
      {loading ? (
        <div className="h-16 w-80 rounded bg-surface-tertiary/50 animate-pulse" />
      ) : (
        <div className="flex items-baseline gap-4 font-extrabold text-fg tracking-tight">
          <span className="text-[52px] leading-none">{formatTokens(input)}</span>
          <span className="text-[36px] text-brand leading-none">→</span>
          <span className="text-[52px] leading-none">{formatTokens(output)}</span>
        </div>
      )}
      <div className="mt-4 flex items-center gap-6 text-xs text-fg-muted uppercase tracking-wider">
        <span>Input</span>
        <span className="opacity-0">·</span>
        <span>Output</span>
      </div>
    </div>
  );
}
