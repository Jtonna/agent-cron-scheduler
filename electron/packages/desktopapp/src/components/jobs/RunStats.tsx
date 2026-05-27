import { formatDuration, formatTimeAgo } from "@/apis/format";
import type { JobRun } from "@/apis/types";

/**
 * RunStats
 *
 * Compact single-line metadata strip rendered beneath the RunHeader on the
 * run-detail page. Dot-separated label/value pairs in muted text — meant
 * to read as quiet supporting context, not as a dashboard. Pending or
 * zero values fall back to "—".
 */

export interface RunStatsProps {
  run: JobRun;
}

function formatCost(cost: number | null | undefined): string {
  if (cost == null || cost <= 0) return "—";
  return `$${cost.toFixed(3)}`;
}

function formatTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

interface StatProps {
  label: string;
  value: React.ReactNode;
}

function Stat({ label, value }: StatProps) {
  return (
    <span className="inline-flex items-baseline gap-1.5 whitespace-nowrap">
      <span className="text-fg-subtle">{label}</span>
      <span className="font-semibold text-fg-secondary">{value}</span>
    </span>
  );
}

export function RunStats({ run }: RunStatsProps) {
  const duration =
    run.total_duration_ms > 0 ? formatDuration(run.total_duration_ms) : "—";

  return (
    <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs text-fg-muted">
      <Stat label="Started" value={formatTimeAgo(run.started_at)} />
      <span className="text-fg-faint" aria-hidden>
        ·
      </span>
      <Stat label="Duration" value={duration} />
      <span className="text-fg-faint" aria-hidden>
        ·
      </span>
      <Stat
        label="Cost"
        value={<span className="font-mono">{formatCost(run.total_cost_usd)}</span>}
      />
      <span className="text-fg-faint" aria-hidden>
        ·
      </span>
      <Stat
        label="Tokens"
        value={
          <span className="font-mono">
            {formatTokens(run.total_input_tokens)} → {formatTokens(run.total_output_tokens)}
          </span>
        }
      />
    </div>
  );
}
