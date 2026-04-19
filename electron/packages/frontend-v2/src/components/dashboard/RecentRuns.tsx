"use client";

import { Badge, statusToBadgeVariant } from "@/components/ui/badge";
import { formatDate, formatCost } from "@/lib/format";
import { useRecentRuns } from "@/hooks/useRecentRuns";
import type { RecentRunEntry } from "@/lib/types";

interface RecentRunsProps {
  onRefresh?: () => void;
}

function formatDuration(durationMs: number | null | undefined): string {
  if (durationMs === null || durationMs === undefined) return "--";
  const totalSeconds = Math.floor(durationMs / 1000);
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return seconds > 0 ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

interface RunCardProps {
  run: RecentRunEntry;
}

function RunCard({ run }: RunCardProps) {
  const badgeVariant = statusToBadgeVariant(run.status, run.exit_code);

  const exitCodeClass =
    run.exit_code === null
      ? "text-muted-foreground"
      : run.exit_code === 0
        ? "text-success"
        : "text-error";

  return (
    <div className="rounded-xl bg-card p-5 shadow-[var(--shadow-card)] border border-border/50 flex flex-col gap-3">
      {/* Header: job name + status badge */}
      <div className="flex items-start justify-between gap-2 min-w-0">
        <span className="font-semibold text-sm truncate flex-1" title={run.job_name}>
          {run.job_name}
        </span>
        <Badge variant={badgeVariant} className="shrink-0">
          {run.status}
        </Badge>
      </div>

      {/* 2x2 info grid */}
      <dl className="grid grid-cols-2 gap-x-4 gap-y-1.5 text-xs">
        <div>
          <dt className="text-muted-foreground">Started</dt>
          <dd className="text-foreground">{formatDate(run.started_at)}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Duration</dt>
          <dd className="text-foreground">{formatDuration(run.duration_ms)}</dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Exit code</dt>
          <dd className={exitCodeClass}>
            {run.exit_code === null ? "--" : run.exit_code}
          </dd>
        </div>
        <div>
          <dt className="text-muted-foreground">Cost</dt>
          <dd className="text-foreground">{formatCost(run.total_cost_usd)}</dd>
        </div>
      </dl>
    </div>
  );
}

function SkeletonCard() {
  return (
    <div
      className="rounded-xl bg-card p-5 shadow-[var(--shadow-card)] border border-border/50 flex flex-col gap-3 animate-pulse"
      aria-hidden="true"
    >
      <div className="flex items-start justify-between gap-2">
        <div className="h-4 w-2/3 rounded bg-muted" />
        <div className="h-5 w-16 rounded-full bg-muted" />
      </div>
      <div className="grid grid-cols-2 gap-x-4 gap-y-1.5">
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className="space-y-1">
            <div className="h-3 w-12 rounded bg-muted" />
            <div className="h-3 w-16 rounded bg-muted" />
          </div>
        ))}
      </div>
    </div>
  );
}

export function RecentRuns({ onRefresh: _onRefresh }: RecentRunsProps = {}) {
  const { runs, loading, error } = useRecentRuns(12);

  return (
    <section aria-labelledby="recent-runs-heading">
      <h2
        id="recent-runs-heading"
        className="text-lg font-semibold mb-4"
      >
        Recent Runs
      </h2>

      {error && (
        <p className="text-sm text-destructive" role="alert">
          {error}
        </p>
      )}

      {loading && !error && (
        <div
          className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-4"
          data-testid="recent-runs-skeleton"
        >
          {Array.from({ length: 8 }).map((_, i) => (
            <SkeletonCard key={i} />
          ))}
        </div>
      )}

      {!loading && !error && runs.length === 0 && (
        <p className="text-sm text-muted-foreground">No recent runs</p>
      )}

      {!loading && !error && runs.length > 0 && (
        <div
          className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 xl:grid-cols-4 gap-4"
          data-testid="recent-runs-grid"
        >
          {runs.map((run) => (
            <RunCard key={run.run_id} run={run} />
          ))}
        </div>
      )}
    </section>
  );
}
