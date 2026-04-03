import React, { useState, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { Badge, statusToBadgeVariant } from "@/components/ui/badge";
import { JobRun } from "@/lib/types";
import { formatCost, formatDate } from "@/lib/format";

interface DayRunsModalProps {
  open: boolean;
  onClose: () => void;
  date: string;
  runs: JobRun[];
  jobId: string;
  totalRunsLoaded: number;
  totalRunsAvailable: number;
}

type SortField = "cost" | "started_at";
type SortDirection = "asc" | "desc";

interface SortState {
  field: SortField;
  direction: SortDirection;
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.round(ms / 1000);
  if (totalSeconds < 60) {
    return `${totalSeconds}s`;
  }
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  if (hours > 0) {
    return seconds > 0
      ? `${hours}h ${minutes}m ${seconds}s`
      : `${hours}h ${minutes}m`;
  }
  return seconds > 0 ? `${minutes}m ${seconds}s` : `${minutes}m`;
}

function formatModalDate(dateStr: string): string {
  const [year, month, day] = dateStr.split("-").map(Number);
  const date = new Date(year, month - 1, day);
  return date.toLocaleDateString("en-US", {
    month: "long",
    day: "numeric",
    year: "numeric",
  });
}

export function DayRunsModal({
  open,
  onClose,
  date,
  runs,
  jobId,
  totalRunsLoaded,
  totalRunsAvailable,
}: DayRunsModalProps) {
  const navigate = useNavigate();
  const [sort, setSort] = useState<SortState>({
    field: "cost",
    direction: "desc",
  });

  const totalCost = useMemo(
    () => runs.reduce((sum, run) => sum + (run.total_cost_usd ?? 0), 0),
    [runs]
  );

  const sortedRuns = useMemo(() => {
    const sorted = [...runs].sort((a, b) => {
      if (sort.field === "cost") {
        const aCost = a.total_cost_usd ?? 0;
        const bCost = b.total_cost_usd ?? 0;
        return sort.direction === "desc" ? bCost - aCost : aCost - bCost;
      } else {
        const aTime = new Date(a.started_at).getTime();
        const bTime = new Date(b.started_at).getTime();
        return sort.direction === "desc" ? bTime - aTime : aTime - bTime;
      }
    });
    return sorted;
  }, [runs, sort]);

  function toggleSort(field: SortField) {
    setSort((prev) => {
      if (prev.field === field) {
        return { field, direction: prev.direction === "desc" ? "asc" : "desc" };
      }
      return { field, direction: "desc" };
    });
  }

  function getSortIndicator(field: SortField): string {
    if (sort.field !== field) return "";
    return sort.direction === "desc" ? " ▼" : " ▲";
  }

  function handleRowClick(run: JobRun) {
    onClose();
    navigate(`/jobs/${jobId}/runs/${run.run_id}`);
  }

  const formattedDate = formatModalDate(date);

  return (
    <Dialog open={open} onOpenChange={(isOpen) => { if (!isOpen) onClose(); }}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{formattedDate}</DialogTitle>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          {/* Summary row */}
          <div className="flex items-center gap-4 text-sm text-muted-foreground">
            <span>{runs.length} {runs.length === 1 ? "run" : "runs"}</span>
            <span className="text-foreground/30">|</span>
            <span>
              Total cost:{" "}
              <span className="font-mono font-medium text-foreground">
                {formatCost(totalCost)}
              </span>
            </span>
          </div>

          {/* Partial data indicator */}
          {totalRunsLoaded < totalRunsAvailable && (
            <p className="text-xs text-muted-foreground bg-muted/50 rounded px-3 py-1.5">
              Showing runs from loaded history ({totalRunsLoaded} of{" "}
              {totalRunsAvailable} total)
            </p>
          )}

          {/* Table or empty state */}
          {runs.length === 0 ? (
            <div className="flex items-center justify-center py-10 rounded-lg border border-dashed">
              <p className="text-sm text-muted-foreground">
                No run data loaded for this date
              </p>
            </div>
          ) : (
            <div className="rounded-lg border max-h-96 overflow-auto">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead
                      className="cursor-pointer select-none hover:text-foreground"
                      onClick={() => toggleSort("started_at")}
                    >
                      Time{getSortIndicator("started_at")}
                    </TableHead>
                    <TableHead>Status</TableHead>
                    <TableHead>Duration</TableHead>
                    <TableHead
                      className="cursor-pointer select-none hover:text-foreground text-right"
                      onClick={() => toggleSort("cost")}
                    >
                      Cost{getSortIndicator("cost")}
                    </TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {sortedRuns.map((run) => (
                    <TableRow
                      key={run.run_id}
                      onClick={() => handleRowClick(run)}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          handleRowClick(run);
                        }
                      }}
                      className="cursor-pointer hover:bg-muted/50"
                      role="link"
                      tabIndex={0}
                    >
                      <TableCell className="text-sm">
                        {formatDate(run.started_at)}
                      </TableCell>
                      <TableCell>
                        <Badge
                          variant={statusToBadgeVariant(
                            run.status,
                            run.exit_code
                          )}
                        >
                          {run.status}
                          {run.status === "Completed" &&
                          run.exit_code !== null &&
                          run.exit_code !== 0
                            ? ` (${run.exit_code})`
                            : ""}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-sm">
                        {run.duration_ms != null
                          ? formatDuration(run.duration_ms)
                          : "--"}
                      </TableCell>
                      <TableCell className="font-mono text-right text-sm">
                        {formatCost(run.total_cost_usd)}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}
