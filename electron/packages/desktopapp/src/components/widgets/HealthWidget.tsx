"use client";

import { Activity } from "lucide-react";
import { PieChart, Pie, Cell, ResponsiveContainer } from "recharts";
import { StatWidget } from "@/components/widgets/StatWidget";
import { JobStateIndicator, type JobState } from "@/components/ui/JobStateIndicator";
import type { RecentRunEntry } from "@/apis/types";

/**
 * HealthWidget
 *
 * Donut chart + legend summarizing run outcomes over the past 14 days.
 * Renders an empty ring with a "0" center when there is no recent
 * activity.
 */

/**
 * Status union accepted by this widget. Matches `RecentRunEntry["status"]`
 * — both `JobRun` and `RecentRunEntry` shapes satisfy this constraint.
 */
export type RunStatus = RecentRunEntry["status"];

/** Minimal structural shape this widget reads from each run. */
interface HealthRun {
  started_at: string;
  status: RunStatus;
}

interface HealthWidgetProps {
  runs: HealthRun[];
}

interface Slice {
  key: Extract<JobState, "success" | "failed" | "running" | "killed" | "warning">;
  label: string;
  value: number;
  color: string;
}

const FOURTEEN_DAYS_MS = 14 * 24 * 60 * 60 * 1000;

function computeSlices(runs: HealthRun[]): Slice[] {
  const cutoff = Date.now() - FOURTEEN_DAYS_MS;
  const recent = runs.filter((r) => new Date(r.started_at).getTime() >= cutoff);

  const counts = {
    success: 0,
    failed: 0,
    running: 0,
    killed: 0,
    warning: 0,
  };
  for (const r of recent) {
    switch (r.status) {
      case "Completed":
        counts.success += 1;
        break;
      case "Failed":
        counts.failed += 1;
        break;
      case "Running":
        counts.running += 1;
        break;
      case "Killed":
        counts.killed += 1;
        break;
      case "CompletedWithWarnings":
        counts.warning += 1;
        break;
    }
  }

  return [
    {
      key: "success",
      label: "Success",
      value: counts.success,
      color: "var(--color-status-success-dot)",
    },
    {
      key: "failed",
      label: "Failed",
      value: counts.failed,
      color: "var(--color-status-failed-dot)",
    },
    {
      key: "running",
      label: "Running",
      value: counts.running,
      color: "var(--color-status-running-dot)",
    },
    {
      key: "killed",
      label: "Killed",
      value: counts.killed,
      color: "var(--color-status-killed-dot)",
    },
    {
      key: "warning",
      label: "Warning",
      value: counts.warning,
      color: "var(--color-status-warning-dot)",
    },
  ];
}

export function HealthWidget({ runs }: HealthWidgetProps) {
  // Filter + small loop over <=200 runs is microseconds — no useMemo needed.
  // Computing inline also fixes a stale-cutoff bug (with [runs] deps, the 14-day
  // cutoff was frozen until runs changed; now it slides every render).
  const slices = computeSlices(runs);
  const total = slices.reduce((sum, s) => sum + s.value, 0);
  const chartData = slices.filter((s) => s.value > 0);

  return (
    <StatWidget title="Health (14d)" icon={<Activity size={14} />}>
      <div className="flex items-center gap-4">
        <div className="w-24 h-24 shrink-0 relative">
          {total === 0 ? (
            <div className="w-24 h-24 rounded-full border-4 border-border-subtle flex items-center justify-center">
              <span className="text-fg-subtle text-xs">0</span>
            </div>
          ) : (
            <>
              <ResponsiveContainer width="100%" height="100%">
                <PieChart>
                  <Pie
                    data={chartData}
                    dataKey="value"
                    innerRadius="65%"
                    outerRadius="100%"
                    paddingAngle={2}
                    stroke="none"
                    isAnimationActive={false}
                  >
                    {chartData.map((entry) => (
                      <Cell key={entry.key} fill={entry.color} />
                    ))}
                  </Pie>
                </PieChart>
              </ResponsiveContainer>
              <div className="absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
                <span className="text-display text-xl num text-fg leading-none">{total}</span>
                <span className="text-eyebrow !text-[10px] !text-fg-subtle mt-0.5">runs</span>
              </div>
            </>
          )}
        </div>
        <div className="flex flex-col gap-1.5 flex-1 min-w-0">
          {slices.map((s) => (
            <div key={s.key} className="flex items-center gap-2 text-xs">
              <JobStateIndicator state={s.key} variant="dot" size="sm" pulse={false} />
              <span className="text-fg-muted flex-1 truncate">{s.label}</span>
              <span className="font-mono text-fg">{s.value}</span>
            </div>
          ))}
        </div>
      </div>
    </StatWidget>
  );
}
