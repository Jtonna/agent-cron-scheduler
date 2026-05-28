"use client";

import type { RecentRunEntry } from "@/apis/types";

/**
 * HealthWidget
 *
 * The fleet health "weather widget" — modeled on acs-ui-refresh's
 * `JobHealth` (the weather/health benchmark the user called out).
 *
 * Composition:
 *  - Pastel `[data-mesh]` surface (mist) with the drifting four-blob
 *    radial gradient + baked-in grain.
 *  - Eyebrow header row with a healthy/degraded status pip.
 *  - Massive ink display numeral for the success percentage.
 *  - Three-segment success/warning/failed bar.
 *  - A grid of count stats anchored at the bottom.
 *
 * Replaces the donut + legend. Reads `RecentRunEntry`-shaped runs and
 * filters to the last 14 days client-side.
 */

export type RunStatus = RecentRunEntry["status"];

interface HealthRun {
  started_at: string;
  status: RunStatus;
}

interface HealthWidgetProps {
  runs: HealthRun[];
}

const FOURTEEN_DAYS_MS = 14 * 24 * 60 * 60 * 1000;

function fmt(n: number): string {
  return n.toLocaleString();
}

function countStatuses(runs: HealthRun[]) {
  const cutoff = Date.now() - FOURTEEN_DAYS_MS;
  const recent = runs.filter((r) => new Date(r.started_at).getTime() >= cutoff);
  const counts = { success: 0, failed: 0, running: 0, killed: 0, warning: 0 };
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
  return counts;
}

export function HealthWidget({ runs }: HealthWidgetProps) {
  const c = countStatuses(runs);
  const total = c.success + c.failed + c.running + c.killed + c.warning;
  const safeTotal = Math.max(total, 1);

  const successW = (c.success / safeTotal) * 100;
  const warningW = (c.warning / safeTotal) * 100;
  const failedW = (c.failed / safeTotal) * 100;

  const pct = total === 0 ? 0 : (c.success / safeTotal) * 100;
  const pctLabel = pct.toFixed(pct === 100 || total === 0 ? 0 : 1);
  const showWarning = c.warning > 0;
  const degraded = c.failed >= Math.max(5, Math.ceil(safeTotal * 0.01));

  return (
    <div
      data-mesh="mist"
      className="rounded-card p-6 min-h-[260px] flex flex-col text-[color:var(--color-ink-900)] border border-border-subtle"
    >
      <div className="flex items-center justify-between">
        <div className="text-eyebrow !text-fg-tertiary">Job health &middot; 14d</div>
        <span className="inline-flex items-center gap-1.5 text-[11px] font-mono num text-[color:var(--color-ink-900)]">
          <span
            className={`h-1.5 w-1.5 rounded-full ${
              total === 0
                ? "bg-fg-subtle"
                : degraded
                  ? "bg-[color:var(--color-status-warning-dot)]"
                  : "bg-[color:var(--color-status-success-dot)]"
            }`}
          />
          {total === 0 ? "idle" : degraded ? "degraded" : "healthy"}
        </span>
      </div>

      <div className="mt-4 flex items-baseline gap-2">
        <div className="text-display text-5xl md:text-6xl num text-[color:var(--color-ink-950)] leading-none">
          {pctLabel}
        </div>
        <div className="text-display text-2xl text-fg-tertiary leading-none">%</div>
      </div>
      <div className="mt-1 text-eyebrow !text-fg-tertiary">healthy</div>

      <div
        className="mt-5 h-1.5 w-full rounded-full overflow-hidden bg-black/8 flex"
        role="img"
        aria-label={`${fmt(c.success)} successful, ${fmt(c.warning)} warning, ${fmt(c.failed)} failed of ${fmt(total)} runs`}
      >
        <span
          className="h-full bg-[color:var(--color-status-success-dot)]"
          style={{ width: `${successW}%` }}
        />
        {showWarning && (
          <span
            className="h-full bg-[color:var(--color-status-warning-dot)]"
            style={{ width: `${warningW}%` }}
          />
        )}
        <span
          className="h-full bg-[color:var(--color-status-failed-dot)]"
          style={{ width: `${failedW}%` }}
        />
      </div>

      <div
        className={`mt-auto pt-5 grid gap-3 ${showWarning ? "grid-cols-4" : "grid-cols-3"}`}
      >
        <Stat label="Runs" value={fmt(total)} />
        <Stat label="Success" value={fmt(c.success)} />
        {showWarning && (
          <Stat
            label="Warning"
            value={fmt(c.warning)}
            valueClass="text-[color:var(--color-status-warning)]"
          />
        )}
        <Stat
          label="Failed"
          value={fmt(c.failed)}
          valueClass="text-[color:var(--color-status-failed)]"
        />
      </div>
    </div>
  );
}

function Stat({
  label,
  value,
  valueClass = "",
}: {
  label: string;
  value: string;
  valueClass?: string;
}) {
  return (
    <div>
      <div className="text-eyebrow !text-[9px] !text-fg-tertiary">{label}</div>
      <div
        className={`mt-1 text-lg font-semibold num text-[color:var(--color-ink-900)] ${valueClass}`}
      >
        {value}
      </div>
    </div>
  );
}
