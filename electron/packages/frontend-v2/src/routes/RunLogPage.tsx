"use client";

import { useState, useEffect, useCallback } from "react";
import { Link, useParams, useNavigate } from "react-router-dom";
import {
  ArrowLeftIcon,
  ArrowPathIcon,
  PlayIcon,
  StopIcon,
} from "@heroicons/react/24/outline";
import {
  CheckCircleIcon,
  XCircleIcon,
  ExclamationTriangleIcon,
} from "@heroicons/react/20/solid";
import { useRunLog } from "@/hooks/useRunLog";
import { useJob } from "@/hooks/useJob";
import { useSSEEvents } from "@/hooks/useSSE";
import { Badge, statusToBadgeVariant } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { LogViewer } from "@/components/LogViewer";
import { formatDate, formatBytes, formatCost } from "@/lib/format";
import { api, ApiError } from "@/lib/api";
import type { JobRun } from "@/lib/types";
import { toast } from "sonner";

function statusIcon(status: string, exitCode: number | null) {
  if (status === "Running") {
    return (
      <ArrowPathIcon className="h-4 w-4 animate-spin text-running shrink-0" />
    );
  }
  if (
    status === "Completed" &&
    (exitCode === null || exitCode === 0)
  ) {
    return (
      <CheckCircleIcon className="h-4 w-4 text-success shrink-0" />
    );
  }
  if (status === "CompletedWithWarnings" || status === "Killed") {
    return (
      <ExclamationTriangleIcon className="h-4 w-4 text-warning shrink-0" />
    );
  }
  return <XCircleIcon className="h-4 w-4 text-error shrink-0" />;
}

export function RunLogPage() {
  const { id: jobId, runId } = useParams<{ id: string; runId: string }>();
  const navigate = useNavigate();

  const { job } = useJob(jobId!);

  const [run, setRun] = useState<JobRun | null>(null);
  const [runLoading, setRunLoading] = useState(true);
  const [runError, setRunError] = useState<string | null>(null);
  const [killLoading, setKillLoading] = useState(false);
  const [retryLoading, setRetryLoading] = useState(false);

  const { log, loading: logLoading, error: logError } = useRunLog(jobId!, runId!);

  const isRunning = run?.status === "Running";

  // Fetch the run metadata by searching the runs list
  const fetchRunMeta = useCallback(async () => {
    if (!jobId || !runId) return;
    try {
      const data = await api.listRuns(jobId, 50, 0);
      const found = data.runs.find((r) => r.run_id === runId);
      if (found) {
        setRun(found);
        setRunError(null);
      } else {
        setRunError("Run not found");
      }
    } catch (err) {
      setRunError(
        err instanceof Error ? err.message : "Failed to fetch run"
      );
    } finally {
      setRunLoading(false);
    }
  }, [jobId, runId]);

  useEffect(() => {
    fetchRunMeta();
  }, [fetchRunMeta]);

  // Re-fetch run metadata when a completion SSE event arrives for this run
  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "killed"
        ) {
          try {
            const payload = JSON.parse(event.data);
            if (payload?.run_id === runId || payload?.job_id === jobId) {
              fetchRunMeta();
            }
          } catch {
            // ignore parse errors
          }
        }
      },
      [runId, jobId, fetchRunMeta]
    )
  );

  const handleKill = async () => {
    if (!jobId || !runId) return;
    setKillLoading(true);
    try {
      await api.killRun(jobId, runId);
      toast.success("Run killed");
      fetchRunMeta();
    } catch (err) {
      if (err instanceof ApiError && err.status === 404) {
        toast.info("Run already completed");
        fetchRunMeta();
      } else {
        toast.error(
          `Failed to kill run: ${err instanceof Error ? err.message : "Unknown error"}`
        );
      }
    } finally {
      setKillLoading(false);
    }
  };

  const handleRetry = async () => {
    if (!jobId || !run) return;
    setRetryLoading(true);
    try {
      const result = await api.triggerJob(
        jobId,
        run.trigger_params ?? undefined
      );
      toast.success("Job triggered");
      if (result?.run_id) {
        navigate(`/jobs/${jobId}/runs/${result.run_id}`);
      }
    } catch (err) {
      toast.error(
        `Failed to retry: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setRetryLoading(false);
    }
  };

  // Loading state — wait for run metadata before rendering the page
  if (runLoading && !run) {
    return (
      <div className="flex items-center justify-center py-12">
        <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  // Error / not found state
  if (runError && !run) {
    return (
      <div className="flex flex-col gap-4 p-6">
        <Link to={`/jobs/${jobId}`}>
          <Button variant="ghost" size="sm">
            <ArrowLeftIcon className="h-4 w-4 mr-1.5" />
            Back to Job
          </Button>
        </Link>
        <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
          <p className="text-sm text-destructive">{runError}</p>
        </div>
      </div>
    );
  }

  const jobName = job?.name ?? jobId;
  const shortRunId = runId ? runId.slice(0, 8) : "—";

  return (
    <div className="flex flex-col h-full gap-4 p-6">
      {/* Header bar */}
      <div className="flex items-center gap-3 flex-wrap">
        {/* Back button */}
        <Link to={`/jobs/${jobId}`}>
          <Button variant="ghost" size="sm" className="gap-1.5">
            <ArrowLeftIcon className="h-4 w-4" />
            Back
          </Button>
        </Link>

        {/* Breadcrumb */}
        <div className="flex items-center gap-1.5 text-sm text-muted-foreground min-w-0">
          <Link
            to={`/jobs/${jobId}`}
            className="hover:text-foreground truncate max-w-[200px] transition-colors"
          >
            {jobName}
          </Link>
          <span className="text-muted-foreground/50 select-none">/</span>
          <span className="font-mono text-foreground">
            Run {shortRunId}
          </span>
        </div>

        {/* Run status badge */}
        {run && (
          <div className="flex items-center gap-1.5 ml-1">
            {statusIcon(run.status, run.exit_code)}
            <Badge
              variant={statusToBadgeVariant(run.status, run.exit_code)}
              className="text-xs"
            >
              {run.status}
            </Badge>
          </div>
        )}

        {/* Action buttons */}
        <div className="ml-auto flex items-center gap-2">
          {/* Kill button — only for running jobs */}
          {isRunning && (
            <Button
              size="sm"
              variant="destructive"
              onClick={handleKill}
              disabled={killLoading}
              className="gap-1.5"
            >
              {killLoading ? (
                <ArrowPathIcon className="h-4 w-4 animate-spin" />
              ) : (
                <StopIcon className="h-4 w-4" />
              )}
              Kill
            </Button>
          )}

          {/* Re-run button */}
          <Button
            size="sm"
            variant="outline"
            onClick={handleRetry}
            disabled={isRunning || retryLoading}
            className="gap-1.5"
          >
            {retryLoading ? (
              <ArrowPathIcon className="h-4 w-4 animate-spin" />
            ) : (
              <PlayIcon className="h-4 w-4" />
            )}
            Re-run
          </Button>
        </div>
      </div>

      {/* Metadata strip */}
      {run && (
        <div className="flex flex-wrap items-center gap-x-4 gap-y-1 px-3 py-2 rounded-lg border bg-muted/30 text-sm">
          {/* Run ID */}
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground text-xs">Run</span>
            <span className="font-mono text-xs" title={run.run_id}>
              {run.run_id.slice(0, 8)}
            </span>
          </div>

          <span className="text-muted-foreground/40 select-none">&middot;</span>

          {/* Start time */}
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground text-xs">Started</span>
            <span className="text-xs">{formatDate(run.started_at)}</span>
          </div>

          {/* End time */}
          {run.finished_at && (
            <>
              <span className="text-muted-foreground/40 select-none">&middot;</span>
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground text-xs">Finished</span>
                <span className="text-xs">{formatDate(run.finished_at)}</span>
              </div>
            </>
          )}

          {/* Duration */}
          {run.duration_ms != null && (
            <>
              <span className="text-muted-foreground/40 select-none">&middot;</span>
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground text-xs">Duration</span>
                <span className="text-xs">
                  {run.duration_ms < 1000
                    ? `${run.duration_ms}ms`
                    : `${(run.duration_ms / 1000).toFixed(1)}s`}
                </span>
              </div>
            </>
          )}

          {/* Exit code */}
          {run.exit_code != null && (
            <>
              <span className="text-muted-foreground/40 select-none">&middot;</span>
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground text-xs">Exit</span>
                <span
                  className={`font-mono text-xs font-medium ${
                    run.exit_code === 0 ? "text-success" : "text-error"
                  }`}
                >
                  {run.exit_code}
                </span>
              </div>
            </>
          )}

          {/* Log size */}
          <span className="text-muted-foreground/40 select-none">&middot;</span>
          <div className="flex items-center gap-1.5">
            <span className="text-muted-foreground text-xs">Size</span>
            <span className="text-xs">{formatBytes(run.log_size_bytes)}</span>
          </div>

          {/* Cost */}
          {run.total_cost_usd != null && (
            <>
              <span className="text-muted-foreground/40 select-none">&middot;</span>
              <div className="flex items-center gap-1.5">
                <span className="text-muted-foreground text-xs">Cost</span>
                <span className="text-xs">{formatCost(run.total_cost_usd)}</span>
              </div>
            </>
          )}
        </div>
      )}

      {/* Error message from run */}
      {run?.error && (
        <div className="px-3 py-2 rounded-lg border border-destructive/30 bg-destructive/5 text-xs text-destructive">
          {run.error}
        </div>
      )}

      {/* LogViewer — fills remaining vertical space */}
      <div className="flex-1 min-h-0">
        <LogViewer
          content={log}
          loading={logLoading && !log}
          error={logError}
          live={isRunning}
          className="h-full"
          maxHeight="100%"
        />
      </div>
    </div>
  );
}
