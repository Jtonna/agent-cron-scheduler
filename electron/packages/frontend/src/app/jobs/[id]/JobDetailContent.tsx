"use client";

import React, { useState, useCallback } from "react";
import Link from "next/link";
import { useParams, useRouter } from "next/navigation";
import {
  ArrowLeftIcon,
  PencilIcon,
  PlayIcon,
  TrashIcon,
  ArrowPathIcon,
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
import { useSSEEvents } from "@/hooks/useSSE";
import { api } from "@/lib/api";
import { formatDate, formatBytes } from "@/lib/format";
import { toast } from "sonner";

export function JobDetailContent() {
  const params = useParams();
  const router = useRouter();
  const id = params.id as string;
  const {
    job,
    loading: jobLoading,
    error: jobError,
    refresh: refreshJob,
  } = useJob(id);
  const {
    runs,
    total,
    loading: runsLoading,
    refresh: refreshRuns,
  } = useRuns(id);
  const [showDelete, setShowDelete] = useState(false);
  const [actionLoading, setActionLoading] = useState(false);

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
          refreshRuns();
        }
      },
      [refreshJob, refreshRuns]
    )
  );

  const handleTrigger = async () => {
    setActionLoading(true);
    try {
      const result = await api.triggerJob(id);
      toast.success("Job triggered");
      refreshRuns();
      if (result?.run_id) {
        router.push(`/jobs/${id}/runs/${result.run_id}`);
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
      await api.deleteJob(id);
      toast.success("Job deleted");
      router.push("/jobs");
    } catch (err) {
      toast.error(
        `Failed to delete: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    } finally {
      setActionLoading(false);
      setShowDelete(false);
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
        <Link href="/jobs">
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
            <Link href={`/jobs/${id}/edit`}>
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
        {runsLoading && runs.length === 0 ? (
          <div className="flex items-center justify-center py-6">
            <ArrowPathIcon className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : runs.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-12 gap-3 rounded-lg border border-dashed">
            <p className="text-sm text-muted-foreground">No runs yet</p>
            <p className="text-xs text-muted-foreground">
              Trigger the job or wait for it to run on schedule.
            </p>
          </div>
        ) : (
          <div className="rounded-lg border">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>Status</TableHead>
                  <TableHead>Started</TableHead>
                  <TableHead>Finished</TableHead>
                  <TableHead>Exit Code</TableHead>
                  <TableHead>Log Size</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {runs.map((run) => (
                  <TableRow
                    key={run.run_id}
                    onClick={() =>
                      router.push(`/jobs/${id}/runs/${run.run_id}`)
                    }
                    onKeyDown={(e) => {
                      if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault();
                        router.push(`/jobs/${id}/runs/${run.run_id}`);
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
                    <TableCell className="text-sm">
                      {formatBytes(run.log_size_bytes)}
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
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
    </div>
  );
}
