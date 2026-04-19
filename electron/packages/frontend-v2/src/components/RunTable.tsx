"use client";

import React from "react";
import { Badge, statusToBadgeVariant } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatDate } from "@/lib/format";
import type { JobRun } from "@/lib/types";

export interface RunTableProps {
  runs: JobRun[];
  totalCount: number;
  loading: boolean;
  hasMore: boolean;
  onLoadMore: () => void;
  onRunClick: (runId: string) => void;
}

function formatDuration(run: JobRun): string {
  // Prefer the explicit duration_ms field if available
  if (run.duration_ms != null) {
    return msToHuman(run.duration_ms);
  }
  // Fall back to computing from timestamps
  if (run.started_at && run.finished_at) {
    const ms =
      new Date(run.finished_at).getTime() - new Date(run.started_at).getTime();
    return msToHuman(ms);
  }
  // Still running — show elapsed time
  if (run.started_at && !run.finished_at) {
    const ms = Date.now() - new Date(run.started_at).getTime();
    return msToHuman(ms);
  }
  return "--";
}

function msToHuman(ms: number): string {
  if (ms < 1000) return "< 1s";
  const totalSeconds = Math.floor(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m ${seconds}s`;
  }
  if (minutes > 0) {
    return `${minutes}m ${seconds}s`;
  }
  return `${seconds}s`;
}

function RunStatusBadge({ run }: { run: JobRun }) {
  const variant = statusToBadgeVariant(run.status, run.exit_code);
  const label =
    run.status === "CompletedWithWarnings" ? "Warnings" : run.status;
  return <Badge variant={variant}>{label}</Badge>;
}

function TableSkeleton() {
  return (
    <div className="rounded-lg border">
      <Table>
        <TableHeader>
          <TableRow className="bg-muted/50">
            <TableHead>Status</TableHead>
            <TableHead>Run ID</TableHead>
            <TableHead>Started</TableHead>
            <TableHead>Duration</TableHead>
            <TableHead>Exit Code</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {Array.from({ length: 5 }).map((_, i) => (
            <TableRow key={i}>
              <TableCell>
                <div className="h-5 w-20 animate-pulse rounded-full bg-muted" />
              </TableCell>
              <TableCell>
                <div className="h-4 w-24 animate-pulse rounded bg-muted font-mono" />
              </TableCell>
              <TableCell>
                <div className="h-4 w-28 animate-pulse rounded bg-muted" />
              </TableCell>
              <TableCell>
                <div className="h-4 w-16 animate-pulse rounded bg-muted" />
              </TableCell>
              <TableCell>
                <div className="h-4 w-8 animate-pulse rounded bg-muted" />
              </TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}

export function RunTable({
  runs,
  totalCount,
  loading,
  hasMore,
  onLoadMore,
  onRunClick,
}: RunTableProps) {
  if (loading && runs.length === 0) {
    return <TableSkeleton />;
  }

  if (!loading && runs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 gap-3">
        <p className="text-sm text-muted-foreground">No runs yet.</p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow className="bg-muted/50">
              <TableHead>Status</TableHead>
              <TableHead>Run ID</TableHead>
              <TableHead>Started</TableHead>
              <TableHead>Duration</TableHead>
              <TableHead>Exit Code</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {runs.map((run) => (
              <TableRow
                key={run.run_id}
                className="cursor-pointer"
                onClick={() => onRunClick(run.run_id)}
              >
                <TableCell>
                  <RunStatusBadge run={run} />
                </TableCell>
                <TableCell>
                  <span className="font-mono text-xs text-foreground hover:underline">
                    {run.run_id.slice(0, 8)}
                  </span>
                </TableCell>
                <TableCell className="text-sm text-muted-foreground">
                  {formatDate(run.started_at)}
                </TableCell>
                <TableCell className="text-sm text-muted-foreground">
                  {formatDuration(run)}
                </TableCell>
                <TableCell>
                  {run.exit_code == null ? (
                    <span className="text-sm text-muted-foreground">--</span>
                  ) : (
                    <span
                      className={`text-sm font-mono ${
                        run.exit_code !== 0
                          ? "text-error font-semibold"
                          : "text-muted-foreground"
                      }`}
                    >
                      {run.exit_code}
                    </span>
                  )}
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {(hasMore || loading) && (
        <div className="flex items-center justify-between px-1">
          <p className="text-xs text-muted-foreground">
            Showing {runs.length} of {totalCount} runs
          </p>
          <Button
            variant="outline"
            size="sm"
            onClick={onLoadMore}
            disabled={loading || !hasMore}
          >
            {loading ? "Loading..." : "Load More"}
          </Button>
        </div>
      )}
    </div>
  );
}
