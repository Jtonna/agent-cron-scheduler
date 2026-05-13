"use client";

import { useMemo } from "react";
import Link from "next/link";
import { TrendingUp } from "lucide-react";
import { StatWidget } from "@/components/widgets/StatWidget";
import type { WorkflowCostEntry } from "@/apis/types";

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
    <StatWidget title="Top spenders" icon={<TrendingUp size={14} />}>
      {top.length === 0 ? (
        <div className="text-fg-subtle text-sm py-2">No cost data yet</div>
      ) : (
        <ol className="flex flex-col gap-2">
          {top.map((entry, idx) => {
            const cost = entry.cost_summary.last_30_days_total_usd;
            const runs = entry.cost_summary.last_30_days_runs;
            return (
              <li key={entry.workflow_id} className="flex items-center gap-2 text-sm">
                <span className="w-4 text-fg-subtle font-mono text-xs shrink-0">{idx + 1}</span>
                <Link
                  href={`/jobs/${entry.workflow_id}`}
                  className="text-fg hover:text-brand truncate flex-1 min-w-0"
                >
                  {entry.workflow_name}
                </Link>
                <span className="font-mono text-xs text-fg-muted shrink-0">
                  {formatUsd(cost)}
                </span>
                <span className="text-xs text-fg-subtle shrink-0 w-12 text-right">
                  {runs} {runs === 1 ? "run" : "runs"}
                </span>
              </li>
            );
          })}
        </ol>
      )}
    </StatWidget>
  );
}
