import React, { useState, useCallback, useEffect } from "react";
import { Link, useNavigate, useParams } from "react-router-dom";
import {
  ArrowLeftIcon,
  PencilIcon,
  PlayIcon,
  TrashIcon,
  ArrowPathIcon,
  PlusIcon,
  XMarkIcon,
  StopIcon,
} from "@heroicons/react/24/outline";
import { useJob } from "@/hooks/useJob";
import { useRuns } from "@/hooks/useRuns";
import {
  Badge,
  statusToBadgeVariant,
} from "@/components/ui/badge";
import { JobStatusBadge } from "@/lib/job-status";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { Label } from "@/components/ui/label";
import { useSSEEvents } from "@/hooks/useSSE";
import { api, ApiError } from "@/lib/api";
import { formatDate, formatBytes, formatCost } from "@/lib/format";
import { toast } from "sonner";
import type { JobRun, TriggerParams } from "@/lib/types";
import { CostPerRunChart } from "@/components/CostPerRunChart";

const PAGE_SIZE = 15;

export function JobDetailPage() {
  const navigate = useNavigate();
  const { id } = useParams<{ id: string }>();
  const {
    job,
    loading: jobLoading,
    error: jobError,
    refresh: refreshJob,
  } = useJob(id!);

  const [page, setPage] = useState(0);
  const [allRuns, setAllRuns] = useState<JobRun[]>([]);
  const [loadingMore, setLoadingMore] = useState(false);

  const {
    runs,
    total,
    loading: runsLoading,
    refresh: refreshRuns,
  } = useRuns(id!, PAGE_SIZE, 0);

  const [showDelete, setShowDelete] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

  // Per-run action loading state: maps run_id -> "killing" | "rerunning" | null
  const [runActionLoading, setRunActionLoading] = useState<Record<string, "killing" | "rerunning">>({});

  // Trigger with overrides state
  const [showTriggerOverrides, setShowTriggerOverrides] = useState(false);
  const [triggerArgs, setTriggerArgs] = useState("");
  const [triggerEnv, setTriggerEnv] = useState<Array<{ key: string; value: string }>>([
    { key: "", value: "" },
  ]);
  const [triggerInput, setTriggerInput] = useState("");

  // When runs from the hook changes and we're on page 0, replace allRuns
  useEffect(() => {
    if (page === 0 && runs.length > 0) {
      setAllRuns(runs);
    }
  }, [runs, page]);

  // Reset page on SSE events
  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "started"
        ) {
          refreshJob();
          setPage(0);
          refreshRuns();
        }
      },
      [refreshJob, refreshRuns]
    )
  );

  const handleLoadMore = async () => {
    setLoadingMore(true);
    try {
      const nextPage = page + 1;
      const nextOffset = nextPage * PAGE_SIZE;
      const data = await api.listRuns(id!, PAGE_SIZE, nextOffset);
      setAllRuns((prev) => [...prev, ...data.runs]);
      setPage(nextPage);
    } catch (err) {
      toast.error(
        `Failed to load more: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setLoadingMore(false);
    }
  };

  const handleTrigger = async () => {
    setActionLoading(true);
    try {
      const result = await api.triggerJob(id!);
      toast.success("Job triggered");
      refreshRuns();
      if (result?.run_id) {
        navigate(`/jobs/${id}/runs/${result.run_id}`);
      }
    } catch (err) {
      toast.error(
        `Failed to trigger: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setActionLoading(false);
    }
  };

  const handleDelete = async () => {
    setActionLoading(true);
    try {
      await api.deleteJob(id!);
      toast.success("Job deleted");
      navigate("/jobs");
    } catch (err) {
      toast.error(
        `Failed to delete: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setActionLoading(false);
      setShowDelete(false);
    }
  };

  const handleTriggerWithOverrides = async () => {
    setActionLoading(true);
    try {
      const params: TriggerParams = {};

      // Add args if not empty
      if (triggerArgs.trim()) {
        params.args = triggerArgs.trim();
      }

      // Add env if there are non-empty key pairs
      const envPairs = triggerEnv.filter((pair) => pair.key.trim() !== "");
      if (envPairs.length > 0) {
        params.env = envPairs.reduce((acc, pair) => {
          acc[pair.key] = pair.value;
          return acc;
        }, {} as Record<string, string>);
      }

      // Add input if not empty
      if (triggerInput.trim()) {
        params.input = triggerInput.trim();
      }

      const result = await api.triggerJob(id!, params);
      toast.success("Job triggered with overrides");
      setShowTriggerOverrides(false);
      resetTriggerForm();
      refreshRuns();
      if (result?.run_id) {
        navigate(`/jobs/${id}/runs/${result.run_id}`);
      }
    } catch (err) {
      toast.error(
        `Failed to trigger: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setActionLoading(false);
    }
  };

  const resetTriggerForm = () => {
    setTriggerArgs("");
    setTriggerEnv([{ key: "", value: "" }]);
    setTriggerInput("");
  };

  const addEnvRow = () => {
    setTriggerEnv([...triggerEnv, { key: "", value: "" }]);
  };

  const removeEnvRow = (index: number) => {
    setTriggerEnv(triggerEnv.filter((_, i) => i !== index));
  };

  const updateEnvRow = (index: number, field: "key" | "value", value: string) => {
    const updated = [...triggerEnv];
    updated[index][field] = value;
    setTriggerEnv(updated);
  };

  const handleKillRun = async (e: React.MouseEvent, runId: string) => {
    e.stopPropagation();
    setRunActionLoading((prev) => ({ ...prev, [runId]: "killing" }));
    try {
      await api.killRun(id!, runId);
      toast.success("Kill signal sent");
      refreshRuns();
    } catch (err) {
      // Handle the case where the run is no longer active (404)
      if (err instanceof ApiError && err.status === 404) {
        toast.info("Run is no longer active — refreshing status");
        refreshRuns();
      } else {
        toast.error(
          `Failed to kill run: ${err instanceof Error ? err.message : "Unknown error"}`
        );
      }
    } finally {
      setRunActionLoading((prev) => {
        const next = { ...prev };
        delete next[runId];
        return next;
      });
    }
  };

  const handleRerunRun = async (e: React.MouseEvent, run: JobRun) => {
    e.stopPropagation();
    setRunActionLoading((prev) => ({ ...prev, [run.run_id]: "rerunning" }));
    try {
      const result = await api.triggerJob(
        id!,
        run.trigger_params ?? undefined
      );
      toast.success("Job re-triggered");
      refreshRuns();
      if (result?.run_id) {
        navigate(`/jobs/${id}/runs/${result.run_id}`);
      }
    } catch (err) {
      toast.error(
        `Failed to re-trigger: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setRunActionLoading((prev) => {
        const next = { ...prev };
        delete next[run.run_id];
        return next;
      });
    }
  };

  if (jobLoading) {
    return (
      <div className="flex items-center justify-center py-12">
        <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
      </div>
    );
  }

  if (jobError || !job) {
    return (
      <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4">
        <p className="text-sm text-destructive">
          {jobError || "Job not found"}
        </p>
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-6 w-full">
      {/* Back navigation */}
      <Button variant="ghost" size="sm" asChild>
        <Link to="/jobs">
          <ArrowLeftIcon className="h-4 w-4 mr-1.5" />
          Back to Jobs
        </Link>
      </Button>

      {/* Header: name + status + actions */}
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex items-center gap-3">
          <h1 className="text-2xl font-semibold tracking-tight">{job.name}</h1>
          <JobStatusBadge
            enabled={job.enabled}
            last_exit_code={job.last_exit_code}
            last_run_at={job.last_run_at}
          />
        </div>
        <div className="flex gap-2">
          <Button variant="secondary" size="sm" asChild>
            <Link to={`/jobs/${id}/edit`}>
              <PencilIcon className="h-4 w-4 mr-1.5" />
              Edit
            </Link>
          </Button>
          <Button
            size="sm"
            onClick={handleTrigger}
            disabled={actionLoading}
          >
            {actionLoading ? (
              <ArrowPathIcon className="h-4 w-4 mr-1.5 animate-spin" />
            ) : (
              <PlayIcon className="h-4 w-4 mr-1.5" />
            )}
            Trigger
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => setShowTriggerOverrides(true)}
            disabled={actionLoading}
          >
            <PlayIcon className="h-4 w-4 mr-1.5" />
            Trigger with Overrides
          </Button>
          <Button
            variant="destructive"
            size="sm"
            onClick={() => setShowDelete(true)}
          >
            <TrashIcon className="h-4 w-4 mr-1.5" />
            Delete
          </Button>
        </div>
      </div>

      {/* Compact Configuration Summary */}
      <Card>
        <CardHeader className="pb-3">
          <CardTitle className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
            Configuration
          </CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-2 sm:grid-cols-3 gap-x-6 gap-y-3">
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Schedule
              </span>
              <span className="font-mono text-sm">{job.schedule}</span>
            </div>
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Type
              </span>
              <span className="text-sm">
                {job.execution.type === "ShellCommand"
                  ? "Shell Command"
                  : "Script File"}
              </span>
            </div>
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Enabled
              </span>
              <Badge variant={job.enabled ? "success" : "disabled"}>
                {job.enabled ? "Enabled" : "Disabled"}
              </Badge>
            </div>
            <div className="flex flex-col gap-0.5 col-span-2 sm:col-span-3">
              <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Command
              </span>
              <code className="font-mono text-sm bg-muted px-2 py-1 rounded truncate">
                {job.execution.value}
              </code>
            </div>
            <div className="flex flex-col gap-0.5">
              <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                Schedule Mode
              </span>
              <span className="text-sm">
                {job.schedule_mode === "WaitForCompletion"
                  ? "Wait for Completion"
                  : "Cron"}
              </span>
            </div>
            {job.timezone && (
              <div className="flex flex-col gap-0.5">
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Timezone
                </span>
                <span className="text-sm">{job.timezone}</span>
              </div>
            )}
            {job.next_run_at && (
              <div className="flex flex-col gap-0.5">
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Next Run
                </span>
                <span className="text-sm">{formatDate(job.next_run_at)}</span>
              </div>
            )}
            {job.pre_hook && (
              <div className="flex flex-col gap-0.5">
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Pre-hook
                </span>
                <code className="font-mono text-sm bg-muted px-2 py-1 rounded truncate" title={job.pre_hook}>
                  {job.pre_hook}
                </code>
              </div>
            )}
            {job.post_hook && (
              <div className="flex flex-col gap-0.5">
                <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                  Post-hook
                </span>
                <code className="font-mono text-sm bg-muted px-2 py-1 rounded truncate" title={job.post_hook}>
                  {job.post_hook}
                </code>
              </div>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Run History — main focus */}
      <div className="flex flex-col gap-3">
        <h2 className="text-lg font-medium">
          Run History{" "}
          <span className="text-sm text-muted-foreground font-normal">
            ({total})
          </span>
        </h2>
        {Array.isArray(allRuns) && <CostPerRunChart jobId={id!} allRuns={allRuns} totalRuns={total} />}
        {runsLoading && allRuns.length === 0 ? (
          <div className="flex items-center justify-center py-6">
            <ArrowPathIcon className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : allRuns.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 gap-3 rounded-lg border border-dashed">
            <p className="text-sm text-muted-foreground">No runs yet</p>
            <p className="text-xs text-muted-foreground">
              Trigger the job or wait for it to run on schedule.
            </p>
          </div>
        ) : (
          <>
            <div className="rounded-lg border">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead>Status</TableHead>
                    <TableHead>Started</TableHead>
                    <TableHead>Finished</TableHead>
                    <TableHead>Exit Code</TableHead>
                    <TableHead>Cost</TableHead>
                    <TableHead>Log Size</TableHead>
                    <TableHead className="w-16">Actions</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {allRuns.map((run) => (
                    <TableRow
                      key={run.run_id}
                      onClick={() =>
                        navigate(`/jobs/${id}/runs/${run.run_id}`)
                      }
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          navigate(`/jobs/${id}/runs/${run.run_id}`);
                        }
                      }}
                      className="cursor-pointer hover:bg-muted/50"
                      role="link"
                      tabIndex={0}
                    >
                      <TableCell>
                        <Badge variant={statusToBadgeVariant(run.status, run.exit_code)}>
                          {run.status}{run.status === "Completed" && run.exit_code !== null && run.exit_code !== 0 ? ` (${run.exit_code})` : ""}
                        </Badge>
                      </TableCell>
                      <TableCell className="text-sm">
                        {formatDate(run.started_at)}
                      </TableCell>
                      <TableCell className="text-sm">
                        {formatDate(run.finished_at)}
                      </TableCell>
                      <TableCell className="font-mono text-sm">
                        {run.exit_code ?? "--"}
                      </TableCell>
                      <TableCell className="font-mono text-right text-sm">
                        {formatCost(run.total_cost_usd)}
                      </TableCell>
                      <TableCell className="text-sm">
                        {formatBytes(run.log_size_bytes)}
                      </TableCell>
                      <TableCell onClick={(e) => e.stopPropagation()}>
                        {run.status === "Running" ? (
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7 text-destructive hover:text-destructive hover:bg-destructive/10"
                            onClick={(e) => handleKillRun(e, run.run_id)}
                            disabled={!!runActionLoading[run.run_id]}
                            title="Kill run"
                          >
                            {runActionLoading[run.run_id] === "killing" ? (
                              <ArrowPathIcon className="h-3.5 w-3.5 animate-spin" />
                            ) : (
                              <StopIcon className="h-3.5 w-3.5" />
                            )}
                          </Button>
                        ) : run.status === "Completed" ||
                          run.status === "CompletedWithWarnings" ||
                          run.status === "Failed" ||
                          run.status === "Killed" ? (
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-7 w-7 text-muted-foreground hover:text-foreground"
                            onClick={(e) => handleRerunRun(e, run)}
                            disabled={!!runActionLoading[run.run_id]}
                            title="Re-run"
                          >
                            {runActionLoading[run.run_id] === "rerunning" ? (
                              <ArrowPathIcon className="h-3.5 w-3.5 animate-spin" />
                            ) : (
                              <ArrowPathIcon className="h-3.5 w-3.5" />
                            )}
                          </Button>
                        ) : null}
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>
            {allRuns.length < total && (
              <div className="flex justify-center">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={handleLoadMore}
                  disabled={loadingMore}
                >
                  {loadingMore ? (
                    <>
                      <ArrowPathIcon className="h-4 w-4 mr-1.5 animate-spin" />
                      Loading...
                    </>
                  ) : (
                    "Load More"
                  )}
                </Button>
              </div>
            )}
          </>
        )}
      </div>

      {/* Delete Confirmation */}
      <AlertDialog open={showDelete} onOpenChange={setShowDelete}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Job</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete <strong>{job.name}</strong>? This
              will remove the job and all its run history.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              disabled={actionLoading}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {actionLoading ? "Deleting..." : "Delete"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>

      {/* Trigger with Overrides Modal */}
      <Dialog
        open={showTriggerOverrides}
        onOpenChange={(open) => {
          setShowTriggerOverrides(open);
          if (!open) {
            resetTriggerForm();
          }
        }}
      >
        <DialogContent className="max-w-2xl">
          <DialogHeader>
            <DialogTitle>Trigger with Overrides</DialogTitle>
            <DialogDescription>
              Override parameters for this run only. These do not modify the job
              definition.
            </DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-4">
            {/* Additional Arguments */}
            <div className="flex flex-col gap-2">
              <Label htmlFor="trigger-args">Additional Arguments</Label>
              <Input
                id="trigger-args"
                value={triggerArgs}
                onChange={(e) => setTriggerArgs(e.target.value)}
                placeholder="e.g. --verbose --dry-run"
                className="font-mono text-sm"
              />
              <p className="text-xs text-muted-foreground">
                Extra command-line arguments to append
              </p>
            </div>

            {/* Environment Variables */}
            <div className="flex flex-col gap-2">
              <Label>Environment Variables</Label>
              <div className="flex flex-col gap-2">
                {triggerEnv.map((pair, index) => (
                  <div key={index} className="flex gap-2 items-center">
                    <Input
                      value={pair.key}
                      onChange={(e) => updateEnvRow(index, "key", e.target.value)}
                      placeholder="KEY"
                      className="font-mono text-sm flex-1"
                    />
                    <Input
                      value={pair.value}
                      onChange={(e) => updateEnvRow(index, "value", e.target.value)}
                      placeholder="value"
                      className="font-mono text-sm flex-1"
                    />
                    <Button
                      variant="ghost"
                      size="sm"
                      onClick={() => removeEnvRow(index)}
                      disabled={triggerEnv.length === 1}
                    >
                      <XMarkIcon className="h-4 w-4" />
                    </Button>
                  </div>
                ))}
                <Button
                  variant="outline"
                  size="sm"
                  onClick={addEnvRow}
                  className="self-start"
                >
                  <PlusIcon className="h-4 w-4 mr-1.5" />
                  Add Variable
                </Button>
              </div>
              <p className="text-xs text-muted-foreground">
                Additional or override environment variables
              </p>
            </div>

            {/* Stdin Input */}
            <div className="flex flex-col gap-2">
              <Label htmlFor="trigger-input">Stdin Input</Label>
              <Textarea
                id="trigger-input"
                value={triggerInput}
                onChange={(e) => setTriggerInput(e.target.value)}
                placeholder="Text to pipe to stdin..."
                className="font-mono text-sm"
                rows={4}
              />
              <p className="text-xs text-muted-foreground">
                Data to send to the command's standard input
              </p>
            </div>
          </div>

          <DialogFooter>
            <Button
              variant="ghost"
              onClick={() => {
                setShowTriggerOverrides(false);
                resetTriggerForm();
              }}
            >
              Cancel
            </Button>
            <Button onClick={handleTriggerWithOverrides} disabled={actionLoading}>
              {actionLoading ? (
                <>
                  <ArrowPathIcon className="h-4 w-4 mr-1.5 animate-spin" />
                  Triggering...
                </>
              ) : (
                <>
                  <PlayIcon className="h-4 w-4 mr-1.5" />
                  Trigger
                </>
              )}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}
