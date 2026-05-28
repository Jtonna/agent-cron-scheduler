"use client";

import { useMemo } from "react";
import Link from "next/link";
import { TrendingUp } from "lucide-react";
import type { WorkflowCostEntry } from "@/apis/types";

/**
 * TopSpendersWidget
 *
 * The "leaderboard" tile — top N workflows by 30-day spend. Pastel
 * "fog" mesh persona surface with a hairline-separated ordered list.
 * Composition matches the rest of the redesigned widget set so the
 * dashboard reads as one family.
 */

interface TopSpendersWidgetProps {
  workflows: WorkflowCostEntry[];
  /** Number of top spenders to display. Defaults to 5. */
  limit?: number;
}

function formatUsd(n: number): string {
  return `$${n.toFixed(2)}`;
}

export function TopSpendersWidget({ workflows, limit = 5 }: TopSpendersWidgetProps) {
  const top = useMemo(() => {
    return [...workflows]
      .filter((w) => w.cost_summary.last_30_days_total_usd > 0)
      .sort(
        (a, b) =>
          b.cost_summary.last_30_days_total_usd - a.cost_summary.last_30_days_total_usd,
      )
      .slice(0, limit);
  }, [workflows, limit]);

  return (
    <div
      data-mesh="fog"
      className="rounded-card p-6 min-h-[260px] flex flex-col text-[color:var(--color-ink-900)] border border-border-subtle"
    >
      <div className="text-eyebrow !text-fg-tertiary inline-flex items-center gap-2 mb-3">
        <TrendingUp size={12} />
        <span>Top spenders &middot; 30d</span>
      </div>

      {top.length === 0 ? (
        <div className="flex-1 flex items-center justify-center text-fg-tertiary text-sm">
          No cost data yet
        </div>
      ) : (
        <ol className="flex flex-col">
          {top.map((entry, idx) => {
            const cost = entry.cost_summary.last_30_days_total_usd;
            const runs = entry.cost_summary.last_30_days_runs;
            const isLast = idx === top.length - 1;
            return (
              <li
                key={entry.workflow_id}
                className={`flex items-center gap-3 py-2 text-sm ${
                  isLast ? "" : "border-b border-black/8"
                }`}
              >
                <span className="w-5 text-fg-tertiary font-mono text-xs shrink-0 num">
                  {String(idx + 1).padStart(2, "0")}
                </span>
                <Link
                  href={`/workflows/${entry.workflow_id}`}
                  className="text-[color:var(--color-ink-900)] hover:underline truncate flex-1 min-w-0"
                >
                  {entry.workflow_name}
                </Link>
                <span className="font-mono text-xs text-[color:var(--color-ink-900)] num shrink-0">
                  {formatUsd(cost)}
                </span>
                <span className="text-[10px] text-fg-tertiary shrink-0 w-12 text-right font-mono num">
                  {runs} {runs === 1 ? "run" : "runs"}
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}
