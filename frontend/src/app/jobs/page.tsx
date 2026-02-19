"use client";

import React, { useCallback } from "react";
import Link from "next/link";
import { ArrowPathIcon, PlusCircleIcon } from "@heroicons/react/24/outline";
import { useJobs } from "@/hooks/useJobs";
import { JobTable } from "@/components/jobs/JobTable";
import { Button } from "@/components/ui/button";
import { useSSEEvents } from "@/hooks/useSSE";
import { toast } from "sonner";

export default function AllJobsPage() {
  const {
    jobs,
    loading,
    error,
    refresh,
    triggerJob,
    enableJob,
    disableJob,
    deleteJob,
  } = useJobs();

  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed"
        ) {
          refresh();
        }
      },
      [refresh]
    )
  );

  const handleTrigger = async (id: string) => {
    try {
      await triggerJob(id);
      toast.success("Job triggered successfully");
    } catch (err) {
      toast.error(
        `Failed to trigger job: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    }
  };

  const handleEnable = async (id: string) => {
    try {
      await enableJob(id);
      toast.success("Job enabled");
    } catch (err) {
      toast.error(
        `Failed to enable job: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    }
  };

  const handleDisable = async (id: string) => {
    try {
      await disableJob(id);
      toast.info("Job disabled");
    } catch (err) {
      toast.error(
        `Failed to disable job: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    }
  };

  const handleDelete = async (id: string) => {
    try {
      await deleteJob(id);
      toast.success("Job deleted");
    } catch (err) {
      toast.error(
        `Failed to delete job: ${err instanceof Error ? err.message : "Unknown error"}`
      );
    }
  };

  return (
    <div className="flex flex-col gap-6 w-full">
      <div className="flex items-center justify-between w-full">
        <h1 className="text-2xl font-semibold tracking-tight">All Jobs</h1>
        <Button asChild>
          <Link href="/jobs/new">
            <PlusCircleIcon className="h-4 w-4 mr-1.5" />
            Create Job
          </Link>
        </Button>
      </div>

      {loading && jobs.length === 0 && (
        <div className="flex items-center justify-center py-12">
          <ArrowPathIcon className="h-8 w-8 animate-spin text-muted-foreground" />
        </div>
      )}

      {error && (
        <div className="rounded-lg border border-destructive/50 bg-destructive/10 p-4 flex items-center justify-between">
          <p className="text-sm text-destructive">{error}</p>
          <Button variant="outline" size="sm" onClick={() => refresh()}>
            <ArrowPathIcon className="h-4 w-4 mr-1.5" />
            Retry
          </Button>
        </div>
      )}

      {!loading && (
        <JobTable
          jobs={jobs}
          onTrigger={handleTrigger}
          onEnable={handleEnable}
          onDisable={handleDisable}
          onDelete={handleDelete}
        />
      )}
    </div>
  );
}
