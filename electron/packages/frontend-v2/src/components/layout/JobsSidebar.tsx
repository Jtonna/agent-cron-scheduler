"use client";

import React, { useState, useEffect, useCallback } from "react";
import { useParams, useLocation, useNavigate, Link } from "react-router-dom";
import {
  PencilIcon,
  PlayIcon,
  TrashIcon,
  ArrowLeftIcon,
  ArrowPathIcon,
  ExclamationTriangleIcon,
  PlusIcon,
} from "@heroicons/react/24/outline";
import {
  CheckCircleIcon as CheckCircleSolid,
  XCircleIcon as XCircleSolid,
} from "@heroicons/react/20/solid";
import { api } from "@/lib/api";
import type { Job, JobRun } from "@/lib/types";
import { formatDate } from "@/lib/format";
import { useSSEEvents } from "@/hooks/useSSE";
import { Button } from "@/components/ui/button";
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip";
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
import { ScrollArea } from "@/components/ui/scroll-area";

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Status dot color for a job based on its enabled state and last exit code. */
function jobStatusColor(job: Job): string {
  if (!job.enabled) return "bg-muted-foreground/50"; // disabled → gray
  if (job.last_exit_code === null) return "bg-muted-foreground/50"; // never run → gray
  if (job.last_exit_code === 0) return "bg-emerald-500"; // success → green
  return "bg-red-500"; // non-zero exit → red
}

/** Status icon for a run entry. */
function runStatusIcon(status: JobRun["status"]) {
  switch (status) {
    case "Running":
      return (
        <ArrowPathIcon className="size-3.5 text-blue-500 animate-spin shrink-0" />
      );
    case "Completed":
      return (
        <CheckCircleSolid className="size-3.5 text-emerald-600 dark:text-emerald-400 shrink-0" />
      );
    case "CompletedWithWarnings":
      return (
        <ExclamationTriangleIcon className="size-3.5 text-amber-500 dark:text-amber-400 shrink-0" />
      );
    case "Failed":
      return (
        <XCircleSolid className="size-3.5 text-red-600 dark:text-red-400 shrink-0" />
      );
    case "Killed":
      return (
        <XCircleSolid className="size-3.5 text-amber-500 dark:text-amber-400 shrink-0" />
      );
    default:
      return (
        <span className="inline-block size-3.5 rounded-full bg-muted-foreground/40 shrink-0" />
      );
  }
}

/** SSE event types that should trigger a data refresh. */
const REFRESH_EVENTS = new Set([
  "job_changed",
  "completed",
  "failed",
  "started",
  "killed",
]);

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

/** List state — shown on /jobs and /jobs/create */
function JobListView() {
  const { pathname } = useLocation();
  const [jobs, setJobs] = useState<Job[]>([]);

  const fetchJobs = useCallback(async () => {
    try {
      const data = await api.listJobs();
      if (Array.isArray(data)) setJobs(data);
    } catch {
      // silently ignore — sidebar is non-critical
    }
  }, []);

  useEffect(() => {
    fetchJobs();
  }, [fetchJobs]);

  useSSEEvents(
    useCallback(
      (event) => {
        if (REFRESH_EVENTS.has(event.type)) fetchJobs();
      },
      [fetchJobs],
    ),
  );

  return (
    <div className="flex flex-col h-full">
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <h2 className="text-sm font-semibold">Jobs</h2>
        <Button variant="outline" size="sm" asChild>
          <Link to="/jobs/create">
            <PlusIcon className="size-4 mr-1" />
            Create
          </Link>
        </Button>
      </div>

      {/* Job list */}
      <ScrollArea className="flex-1">
        <ul className="py-1">
          {jobs.length === 0 && (
            <li className="px-4 py-6 text-xs text-muted-foreground text-center">
              No jobs found
            </li>
          )}
          {jobs.map((job) => {
            const isActive =
              pathname === `/jobs/${job.id}` ||
              pathname.startsWith(`/jobs/${job.id}/`);
            return (
              <li key={job.id}>
                <Link
                  to={`/jobs/${job.id}`}
                  className={`flex items-center gap-2.5 px-4 py-2 text-sm transition-colors hover:bg-accent ${
                    isActive
                      ? "bg-accent text-accent-foreground font-medium"
                      : ""
                  }`}
                >
                  <span
                    className={`inline-block size-2 rounded-full shrink-0 ${jobStatusColor(job)}`}
                    aria-hidden
                  />
                  <span className="truncate flex-1">{job.name}</span>
                  <span className="text-[11px] text-muted-foreground whitespace-nowrap font-mono">
                    {job.schedule}
                  </span>
                </Link>
              </li>
            );
          })}
        </ul>
      </ScrollArea>
    </div>
  );
}

/** Detail state — shown on /jobs/:id, /jobs/:id/edit, /jobs/:id/runs/:runId */
function JobDetailView({ jobId }: { jobId: string }) {
  const navigate = useNavigate();
  const { pathname } = useLocation();
  const params = useParams<{ runId?: string }>();

  const [job, setJob] = useState<Job | null>(null);
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [showDeleteDialog, setShowDeleteDialog] = useState(false);
  const [deleteLoading, setDeleteLoading] = useState(false);
  const [triggerLoading, setTriggerLoading] = useState(false);

  const fetchJob = useCallback(async () => {
    try {
      const data = await api.getJob(jobId);
      setJob(data);
    } catch {
      // job may have been deleted — navigate back
    }
  }, [jobId]);

  const fetchRuns = useCallback(async () => {
    try {
      const data = await api.listRuns(jobId, 10);
      if (data && Array.isArray(data.runs)) setRuns(data.runs);
    } catch {
      // silently ignore
    }
  }, [jobId]);

  useEffect(() => {
    fetchJob();
    fetchRuns();
  }, [fetchJob, fetchRuns]);

  useSSEEvents(
    useCallback(
      (event) => {
        if (REFRESH_EVENTS.has(event.type)) {
          fetchJob();
          fetchRuns();
        }
      },
      [fetchJob, fetchRuns],
    ),
  );

  const handleTrigger = async () => {
    setTriggerLoading(true);
    try {
      await api.triggerJob(jobId);
    } catch {
      // ignore — toast or error handling can be added later
    } finally {
      setTriggerLoading(false);
    }
  };

  const handleDelete = async () => {
    setDeleteLoading(true);
    try {
      await api.deleteJob(jobId);
      setShowDeleteDialog(false);
      navigate("/jobs");
    } catch {
      // ignore
    } finally {
      setDeleteLoading(false);
    }
  };

  return (
    <div className="flex flex-col h-full">
      {/* Back button */}
      <div className="px-4 pt-3 pb-1">
        <Button
          variant="ghost"
          size="sm"
          className="gap-1.5 -ml-2 text-muted-foreground hover:text-foreground"
          onClick={() => navigate("/jobs")}
        >
          <ArrowLeftIcon className="size-4" />
          Back to Jobs
        </Button>
      </div>

      {/* Job name heading */}
      <div className="px-4 pb-3 border-b border-border">
        <h2 className="text-sm font-semibold truncate">
          {job?.name ?? "Loading..."}
        </h2>
      </div>

      {/* Action bar */}
      <div className="flex items-center gap-1 px-4 py-2 border-b border-border">
        <TooltipProvider delayDuration={300}>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                onClick={() => navigate(`/jobs/${jobId}/edit`)}
              >
                <PencilIcon className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom">Edit</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-8"
                onClick={handleTrigger}
                disabled={triggerLoading}
              >
                <PlayIcon className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom">Trigger</TooltipContent>
          </Tooltip>

          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant="ghost"
                size="icon"
                className="size-8 text-destructive hover:text-destructive"
                onClick={() => setShowDeleteDialog(true)}
              >
                <TrashIcon className="size-4" />
              </Button>
            </TooltipTrigger>
            <TooltipContent side="bottom">Delete</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>

      {/* Recent Runs */}
      <div className="px-4 pt-3 pb-1">
        <h3 className="text-xs font-medium text-muted-foreground uppercase tracking-wider">
          Recent Runs
        </h3>
      </div>

      <ScrollArea className="flex-1">
        <ul className="py-1">
          {runs.length === 0 && (
            <li className="px-4 py-4 text-xs text-muted-foreground text-center">
              No runs yet
            </li>
          )}
          {runs.map((run) => {
            const isActive = params.runId === run.run_id;
            return (
              <li key={run.run_id}>
                <Link
                  to={`/jobs/${jobId}/runs/${run.run_id}`}
                  className={`flex items-center gap-2 px-4 py-1.5 text-sm transition-colors hover:bg-accent ${
                    isActive
                      ? "bg-accent text-accent-foreground font-medium"
                      : ""
                  }`}
                >
                  {runStatusIcon(run.status)}
                  <span className="font-mono text-xs truncate">
                    {run.run_id.substring(0, 8)}
                  </span>
                  <span className="ml-auto text-[10px] text-muted-foreground whitespace-nowrap">
                    {formatDate(run.started_at)}
                  </span>
                </Link>
              </li>
            );
          })}
        </ul>
      </ScrollArea>

      {/* Delete confirmation dialog */}
      <AlertDialog open={showDeleteDialog} onOpenChange={setShowDeleteDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Job</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete &ldquo;{job?.name}&rdquo;? This
              action cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              disabled={deleteLoading}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {deleteLoading ? "Deleting..." : "Delete"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export function JobsSidebar() {
  const { pathname } = useLocation();
  const params = useParams<{ id?: string }>();

  // Determine which view to show based on the current route.
  // Detail view when we have a job id (e.g. /jobs/:id, /jobs/:id/edit, /jobs/:id/runs/:runId)
  // List view for /jobs and /jobs/create
  const jobId = params.id;
  const isDetailView = Boolean(jobId);

  return (
    <aside
      className="fixed left-0 w-64 bg-background border-r border-border flex flex-col"
      style={{
        top: "var(--topbar-height, 49px)",
        height: "calc(100vh - var(--topbar-height, 49px))",
      }}
    >
      {isDetailView && jobId ? (
        <JobDetailView jobId={jobId} />
      ) : (
        <JobListView />
      )}
    </aside>
  );
}
