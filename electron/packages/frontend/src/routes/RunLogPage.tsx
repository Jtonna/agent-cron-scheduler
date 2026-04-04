import React, { useEffect, useState, useCallback } from "react";
import { Link, useParams, useNavigate } from "react-router-dom";
import {
  ArrowLeftIcon,
  ArrowPathIcon,
  PlayIcon,
} from "@heroicons/react/24/outline";
import { CheckCircleIcon, XCircleIcon } from "@heroicons/react/20/solid";
import { useRunLog } from "@/hooks/useRunLog";
import { useSSEEvents } from "@/hooks/useSSE";
import { Badge, statusToBadgeVariant } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { LogViewer } from "@/components/LogViewer";
import { StreamJsonViewer } from "@/components/StreamJsonViewer";
import { formatDate, formatBytes, formatCost } from "@/lib/format";
import { api } from "@/lib/api";
import type { JobRun } from "@/lib/types";
import { toast } from "sonner";

export function RunLogPage() {
  const { id: jobId, runId } = useParams<{ id: string; runId: string }>();
  const navigate = useNavigate();

  const [run, setRun] = useState<JobRun | null>(null);
  const [runLoading, setRunLoading] = useState(true);
  const [runError, setRunError] = useState<string | null>(null);
  const [streamedContent, setStreamedContent] = useState<string>("");
  const [retryLoading, setRetryLoading] = useState(false);

  const isRunning = run?.status === "Running";
  const { log, loading: logLoading, error: logError, refresh: refreshLog } = useRunLog(runId!, isRunning ? 3000 : undefined);

  // Fetch the run metadata from the runs list
  const fetchRunMeta = useCallback(async () => {
    try {
      const data = await api.listRuns(jobId!, 50, 0);
      const found = data.runs.find((r) => r.run_id === runId);
      if (found) {
        setRun(found);
        setRunError(null);
      } else {
        setRunError("Run not found");
      }
    } catch (err) {
      setRunError(err instanceof Error ? err.message : "Failed to fetch run");
    } finally {
      setRunLoading(false);
    }
  }, [jobId, runId]);

  useEffect(() => {
    fetchRunMeta();
  }, [fetchRunMeta]);

  // Reset streamed content when runId changes
  useEffect(() => {
    setStreamedContent("");
  }, [runId]);

  // When the polled log updates, trim any prefix of streamedContent that is
  // already covered by log to prevent duplicate display.
  useEffect(() => {
    if (!log) return;
    setStreamedContent((prev) => {
      if (!prev) return prev;
      // If log fully contains streamedContent as a prefix, clear it entirely.
      if (log.endsWith(prev)) return "";
      // Find the longest suffix of log that matches a prefix of streamedContent.
      // Walk from the full length of prev down to 1 to find the largest overlap.
      const maxOverlap = Math.min(prev.length, log.length);
      for (let len = maxOverlap; len > 0; len--) {
        if (log.endsWith(prev.slice(0, len))) {
          return prev.slice(len);
        }
      }
      return prev;
    });
  }, [log]);

  const handleRetry = async () => {
    if (!run || !jobId) return;
    setRetryLoading(true);
    try {
      const result = await api.triggerJob(jobId, run.trigger_params ?? undefined);
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

  // SSE streaming for live output + run status updates
  useSSEEvents(
    useCallback(
      (event) => {
        if (event.type === "output" && runId) {
          try {
            const data = JSON.parse(event.data);
            if (data.data?.run_id === runId) {
              setStreamedContent((prev) => prev + (data.data.data || ""));
            }
          } catch {
            // Ignore parse errors
          }
        }
        if (
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "killed"
        ) {
          try {
            const data = JSON.parse(event.data);
            if (data.data?.job_id === jobId) {
              refreshLog();
              fetchRunMeta();
            }
          } catch {
            // Ignore
          }
        }
      },
      [runId, jobId, refreshLog, fetchRunMeta]
    )
  );

  const isLoading = runLoading || logLoading;

  if (isLoading && !run) {
    return (
      <div className="flex items-center justify-center py-12">
        <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (runError && !run) {
    return (
      <div className="flex flex-col gap-4">
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

  const displayContent = log + streamedContent;

  /**
   * Detect if content is stream-json format (NDJSON with type field)
   * Skips ACS log headers (command echo, environment block) to find the first real JSON line
   */
  function isStreamJson(content: string): boolean {
    if (!content) return false;
    const lines = content.split("\n");
    const limit = Math.min(lines.length, 50);
    for (let i = 0; i < limit; i++) {
      const trimmed = lines[i].trim();
      if (!trimmed) continue;
      // Only attempt to parse lines that look like JSON objects
      if (trimmed[0] !== '{') continue;
      try {
        const parsed = JSON.parse(trimmed);
        return parsed && typeof parsed === "object" && "type" in parsed;
      } catch {
        continue;
      }
    }
    return false;
  }

  function statusIcon(status: string, exitCode: number | null) {
    if (status === "Running") {
      return <ArrowPathIcon className="h-4 w-4 text-primary animate-spin" />;
    }
    if (status === "Completed" && (exitCode === null || exitCode === 0)) {
      return <CheckCircleIcon className="h-4 w-4 text-emerald-600 dark:text-emerald-400" />;
    }
    // Failed, Killed, or Completed with non-zero exit code
    return <XCircleIcon className="h-4 w-4 text-destructive" />;
  }

  return (
    <div className="flex flex-col gap-4 w-full">
      {/* Back navigation */}
      <Link to={`/jobs/${jobId}`}>
        <Button variant="ghost" size="sm">
          <ArrowLeftIcon className="h-4 w-4 mr-1.5" />
          Back to Job
        </Button>
      </Link>

      {/* Compact run details bar */}
      {run && (
        <div className="flex flex-col gap-1">
          <div className="flex items-center gap-2 flex-wrap px-3 py-2 border-b bg-muted/30 rounded-t-lg text-sm">
            {/* Status icon + badge */}
            {statusIcon(run.status, run.exit_code)}
            <Badge variant={statusToBadgeVariant(run.status, run.exit_code)} className="text-xs">
              {run.status}
            </Badge>

            <span className="text-muted-foreground/50 select-none">&middot;</span>

            {/* Run ID (truncated) */}
            <span
              className="font-mono text-xs text-muted-foreground truncate"
              title={run.run_id}
            >
              {run.run_id.slice(0, 8)}
            </span>

            <span className="text-muted-foreground/50 select-none">&middot;</span>

            {/* Started */}
            <span className="text-xs text-muted-foreground">
              Started {formatDate(run.started_at)}
            </span>

            {run.finished_at && (
              <>
                <span className="text-muted-foreground/50 select-none">&middot;</span>
                <span className="text-xs text-muted-foreground">
                  Finished {formatDate(run.finished_at)}
                </span>
              </>
            )}

            <span className="text-muted-foreground/50 select-none">&middot;</span>

            {/* Exit code */}
            <span className="text-xs text-muted-foreground">
              Exit{" "}
              <span className="font-mono font-medium text-foreground">
                {run.exit_code ?? "--"}
              </span>
            </span>

            <span className="text-muted-foreground/50 select-none">&middot;</span>

            {/* Log size */}
            <span className="text-xs text-muted-foreground">
              {formatBytes(run.log_size_bytes)}
            </span>

            {run.total_cost_usd != null && (
              <>
                <span className="text-muted-foreground/50 select-none">&middot;</span>

                {/* Cost */}
                <span className="text-xs text-muted-foreground">
                  {formatCost(run.total_cost_usd)}
                </span>
              </>
            )}

            <div className="ml-auto">
              <Button
                size="sm"
                variant="outline"
                onClick={handleRetry}
                disabled={run.status === "Running" || retryLoading}
              >
                {retryLoading ? (
                  <ArrowPathIcon className="h-4 w-4 mr-1.5 animate-spin" />
                ) : (
                  <PlayIcon className="h-4 w-4 mr-1.5" />
                )}
                Retry
              </Button>
            </div>
          </div>

          {/* Error line (only if present) */}
          {run.error && (
            <div className="px-3 py-1.5 text-xs text-destructive bg-destructive/5 border-b rounded-b-lg">
              {run.error}
            </div>
          )}
        </div>
      )}

      {/* Log viewer */}
      {isStreamJson(displayContent) ? (
        <StreamJsonViewer
          content={displayContent}
          loading={logLoading}
          error={logError}
          live={run?.status === "Running"}
        />
      ) : (
        <LogViewer
          content={displayContent}
          loading={logLoading}
          error={logError}
          live={run?.status === "Running"}
        />
      )}
    </div>
  );
}
