"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Loader2 } from "lucide-react";
import { formatDuration, formatTimeAgo } from "@/apis/format";
import type { JobRun } from "@/apis/types";
import { JobStateIndicator, apiStatusToJobState } from "@/components/ui/JobStateIndicator";
import { TabBar, type FilterOptions, type SortKey } from "@/components/ui/TabBar";
import { KillRunButton } from "./KillRunButton";

/**
 * JobRunsTable
 *
 * Dense, table-style listing of a single job's runs. Toolbar mirrors
 * the shared `TabBar`: a simple "Recent" label on the left and the
 * slide-down advanced filter panel on the right (Sort by, cost,
 * duration, date range). All sort + filter dimensions live in one
 * `FilterOptions` object.
 *
 * Status is conveyed by the leading dot — no separate status column.
 */

interface JobRunsTableProps {
  jobId: string;
  runs: JobRun[];
  loading?: boolean;
}

const SORT_LABELS: Record<SortKey, string> = {
  recent: "Recent",
  oldest: "Oldest",
  longest: "Longest",
  shortest: "Shortest",
  "cost-high": "Cost (high to low)",
  "cost-low": "Cost (low to high)",
};

// Shared grid template for header + rows so columns can never drift. The
// trailing 32px column holds the per-row kill button (empty when not Running).
const GRID_COLS =
  "grid grid-cols-[20px_minmax(0,1.6fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_minmax(0,1fr)_32px] items-center gap-4 px-4";

function formatCost(cost: number | null | undefined): string | null {
  if (cost == null || cost <= 0) return null;
  return `$${cost.toFixed(3)}`;
}

interface ExitCodeProps {
  code: number | null;
}

function ExitCode({ code }: ExitCodeProps) {
  if (code === null) {
    return <span className="font-mono text-xs text-fg-faint">—</span>;
  }
  const cls = code === 0 ? "text-status-success" : "text-status-failed";
  return <span className={`font-mono text-xs ${cls}`}>{code}</span>;
}

function safeNum(n: number | null | undefined): number {
  return n ?? 0;
}

function compareRuns(a: JobRun, b: JobRun, key: SortKey): number {
  switch (key) {
    case "recent":
      return new Date(b.started_at).getTime() - new Date(a.started_at).getTime();
    case "oldest":
      return new Date(a.started_at).getTime() - new Date(b.started_at).getTime();
    case "longest":
      return safeNum(b.total_duration_ms) - safeNum(a.total_duration_ms);
    case "shortest":
      return safeNum(a.total_duration_ms) - safeNum(b.total_duration_ms);
    case "cost-high":
      return safeNum(b.total_cost_usd) - safeNum(a.total_cost_usd);
    case "cost-low":
      return safeNum(a.total_cost_usd) - safeNum(b.total_cost_usd);
  }
}

/** Last step's exit code reflects the run's final state. */
function runExitCode(run: JobRun): number | null {
  if (run.steps.length === 0) return null;
  return run.steps[run.steps.length - 1].exit_code;
}

// Parse a YYYY-MM-DD string from `<input type="date">` as the start of that
// local-day in UTC ms. Returns null if the string is empty or unparseable.
function parseDateStart(dateStr: string | undefined): number | null {
  if (!dateStr) return null;
  const t = new Date(`${dateStr}T00:00:00`).getTime();
  return Number.isFinite(t) ? t : null;
}

function parseDateEnd(dateStr: string | undefined): number | null {
  if (!dateStr) return null;
  // End-of-day exclusive upper bound = next day at 00:00.
  const t = new Date(`${dateStr}T23:59:59.999`).getTime();
  return Number.isFinite(t) ? t : null;
}

function parseFloatOpt(s: string | undefined): number | null {
  if (s == null || s === "") return null;
  const v = Number.parseFloat(s);
  return Number.isFinite(v) ? v : null;
}

function matchesFilters(run: JobRun, f: FilterOptions): boolean {
  const startedMs = new Date(run.started_at).getTime();

  const fromMs = parseDateStart(f.dateFrom);
  if (fromMs !== null && startedMs < fromMs) return false;

  const toMs = parseDateEnd(f.dateTo);
  if (toMs !== null && startedMs > toMs) return false;

  const cost = run.total_cost_usd ?? 0;
  const costMin = parseFloatOpt(f.costMin);
  if (costMin !== null && cost < costMin) return false;

  const costMax = parseFloatOpt(f.costMax);
  if (costMax !== null && cost > costMax) return false;

  // FilterPanel labels duration as seconds.
  const durationSec = (run.total_duration_ms ?? 0) / 1000;
  const durationMin = parseFloatOpt(f.durationMin);
  if (durationMin !== null && durationSec < durationMin) return false;

  const durationMax = parseFloatOpt(f.durationMax);
  if (durationMax !== null && durationSec > durationMax) return false;

  return true;
}

export function JobRunsTable({ jobId, runs, loading }: JobRunsTableProps) {
  const router = useRouter();
  const [filters, setFilters] = useState<FilterOptions>({});

  const sortBy = filters.sortBy ?? "recent";
  const filtered = runs.filter((r) => matchesFilters(r, filters));
  const sorted = [...filtered].sort((a, b) => compareRuns(a, b, sortBy));

  return (
    <div className="flex flex-col gap-4">
      <TabBar
        label={{
          value: sortBy,
          options: SORT_LABELS,
          onChange: (key) =>
            setFilters({ ...filters, sortBy: key as SortKey }),
        }}
        filter={{
          filters,
          onFiltersChange: setFilters,
          hideSortBy: true,
        }}
      />

      {/* Header row */}
      <div
        className={`${GRID_COLS} text-[11px] font-semibold text-fg-subtle uppercase tracking-wider`}
      >
        <span aria-hidden />
        <span>Run</span>
        <span>Started</span>
        <span>Duration</span>
        <span>Cost</span>
        <span>Exit</span>
        <span aria-hidden />
      </div>

      {/* Body */}
      {loading && runs.length === 0 ? (
        <div className="flex items-center justify-center py-12">
          <Loader2 size={20} className="animate-spin text-fg-subtle" />
        </div>
      ) : !loading && sorted.length === 0 ? (
        <div className="text-center py-12 text-fg-subtle text-sm">
          {runs.length === 0 ? "No runs yet" : "No runs match these filters"}
        </div>
      ) : (
        <div className="flex flex-col gap-2">
          {sorted.map((run) => {
            const state = apiStatusToJobState(run.status);
            const cost = formatCost(run.total_cost_usd);
            const href = `/jobs/${jobId}/runs/${run.run_id}`;
            const navigate = () => router.push(href);
            return (
              <div
                key={run.run_id}
                role="link"
                tabIndex={0}
                onClick={navigate}
                onKeyDown={(e) => {
                  if (e.target !== e.currentTarget) return;
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    navigate();
                  }
                }}
                className={`${GRID_COLS} py-3 rounded-input border border-border bg-surface hover:bg-surface-hover hover:border-border-strong transition-colors text-sm outline-none focus-visible:ring-2 focus-visible:ring-brand-ring cursor-pointer`}
              >
                <JobStateIndicator state={state} variant="dot" size="sm" />
                <span className="font-mono text-xs text-fg whitespace-nowrap" title={run.run_id}>
                  {run.run_id}
                </span>
                <span className="text-xs text-fg-muted truncate">
                  {formatTimeAgo(run.started_at)}
                </span>
                <span className="text-xs text-fg-muted truncate">
                  {run.total_duration_ms > 0 ? (
                    formatDuration(run.total_duration_ms)
                  ) : (
                    <span className="text-fg-faint">—</span>
                  )}
                </span>
                <span className="text-xs truncate">
                  {cost ? (
                    <span className="font-mono text-fg-muted">{cost}</span>
                  ) : (
                    <span className="text-fg-faint">—</span>
                  )}
                </span>
                <ExitCode code={runExitCode(run)} />
                {/* Kill button column. Stop propagation so clicking the
                    button doesn't also fire the row's navigate handler. */}
                <span
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => e.stopPropagation()}
                  className="inline-flex items-center justify-center"
                >
                  {run.status === "Running" ? (
                    <KillRunButton runId={run.run_id} size="sm" />
                  ) : null}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
