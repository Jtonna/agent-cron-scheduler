"use client";

/**
 * HeroTokensSlide
 *
 * Hero-sized slide showing the system-wide input → output token flow
 * for the last 30 days. Uses the cool "pulse" pastel persona because
 * tokens are an "energetic" data axis. Ink-black display numerals with
 * the brand-pink arrow keep the dashboard's accent visible across
 * carousel slides.
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
    <div className="gradient-pulse-linear w-full h-full rounded-card flex flex-col items-center justify-center px-8 text-center text-[color:var(--color-ink-900)]">
      <div className="text-eyebrow !text-[color:var(--color-fg-tertiary)] inline-flex items-center gap-2 mb-5">
        <Type size={12} />
        <span>Token flow &middot; last 30 days</span>
      </div>
      {loading ? (
        <div className="h-16 w-80 rounded bg-black/5 animate-pulse" />
      ) : (
        <div className="text-display flex items-baseline gap-5 num text-[color:var(--color-ink-950)]">
          <span className="text-[64px] md:text-[80px]">{formatTokens(input)}</span>
          <span className="text-[44px] md:text-[56px] text-brand">&rarr;</span>
          <span className="text-[64px] md:text-[80px]">{formatTokens(output)}</span>
        </div>
      )}
      <div className="mt-5 flex items-center gap-10 text-eyebrow !text-[color:var(--color-fg-tertiary)]">
        <span>Input</span>
        <span>Output</span>
      </div>
    </div>
  );
}
