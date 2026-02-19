"use client";

import React, { useState } from "react";
import Link from "next/link";
import { PlayIcon, PencilIcon, TrashIcon } from "@heroicons/react/24/outline";
import type { Job } from "@/lib/types";
import { JobStatusBadge } from "@/lib/job-status";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
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
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { formatDate } from "@/lib/format";

type SortField = "name" | "last_run_at";
type SortDir = "asc" | "desc";

interface JobTableProps {
  jobs: Job[];
  onTrigger: (id: string) => Promise<void>;
  onEnable: (id: string) => Promise<void>;
  onDisable: (id: string) => Promise<void>;
  onDelete: (id: string) => Promise<void>;
}

export function JobTable({
  jobs,
  onTrigger,
  onEnable,
  onDisable,
  onDelete,
}: JobTableProps) {
  const [sortField, setSortField] = useState<SortField>("name");
  const [sortDir, setSortDir] = useState<SortDir>("asc");
  const [deleteTarget, setDeleteTarget] = useState<Job | null>(null);
  const [actionLoading, setActionLoading] = useState<string | null>(null);

  if (jobs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-12 gap-3">
        <p className="text-sm text-muted-foreground">No jobs found</p>
        <p className="text-xs text-muted-foreground">
          Create your first job to get started.
        </p>
      </div>
    );
  }

  const sorted = [...jobs].sort((a, b) => {
    let cmp = 0;
    if (sortField === "name") {
      cmp = a.name.localeCompare(b.name);
    } else if (sortField === "last_run_at") {
      const aTime = a.last_run_at ? new Date(a.last_run_at).getTime() : 0;
      const bTime = b.last_run_at ? new Date(b.last_run_at).getTime() : 0;
      cmp = aTime - bTime;
    }
    return sortDir === "asc" ? cmp : -cmp;
  });

  const toggleSort = (field: SortField) => {
    if (sortField === field) {
      setSortDir(sortDir === "asc" ? "desc" : "asc");
    } else {
      setSortField(field);
      setSortDir("asc");
    }
  };

  const SortIndicator = ({ field }: { field: SortField }) => (
    <span
      className={`ml-1 ${sortField === field ? "opacity-100" : "opacity-40"}`}
    >
      {sortField === field && sortDir === "desc" ? "\u25B2" : "\u25BC"}
    </span>
  );

  const handleAction = async (id: string, action: () => Promise<void>) => {
    setActionLoading(id);
    try {
      await action();
    } catch {
      // Error handled by parent
    } finally {
      setActionLoading(null);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    setActionLoading(deleteTarget.id);
    try {
      await onDelete(deleteTarget.id);
      setDeleteTarget(null);
    } catch {
      // Error handled by parent
    } finally {
      setActionLoading(null);
    }
  };

  return (
    <>
      <div className="rounded-lg border">
        <Table>
          <TableHeader>
            <TableRow className="bg-muted/50">
              <TableHead>Status</TableHead>
              <TableHead
                className="cursor-pointer select-none"
                onClick={() => toggleSort("name")}
                aria-sort={sortField === "name" ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
              >
                Name <SortIndicator field="name" />
              </TableHead>
              <TableHead>Schedule</TableHead>
              <TableHead>Type</TableHead>
              <TableHead
                className="cursor-pointer select-none"
                onClick={() => toggleSort("last_run_at")}
                aria-sort={sortField === "last_run_at" ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
              >
                Last Run <SortIndicator field="last_run_at" />
              </TableHead>
              <TableHead>Next Run</TableHead>
              <TableHead className="text-right">Actions</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {sorted.map((job) => (
              <TableRow key={job.id}>
                <TableCell>
                  <JobStatusBadge
                    enabled={job.enabled}
                    last_exit_code={job.last_exit_code}
                    last_run_at={job.last_run_at}
                  />
                </TableCell>
                <TableCell>
                  <Link
                    href={`/jobs/${job.id}`}
                    className="font-medium text-foreground hover:underline"
                  >
                    {job.name}
                  </Link>
                </TableCell>
                <TableCell className="font-mono text-xs">
                  {job.schedule}
                </TableCell>
                <TableCell>
                  {job.execution.type === "ShellCommand" ? "Shell" : "Script"}
                </TableCell>
                <TableCell>{formatDate(job.last_run_at)}</TableCell>
                <TableCell>
                  {job.enabled ? formatDate(job.next_run_at) : "--"}
                </TableCell>
                <TableCell>
                  <div className="flex items-center justify-end gap-1">
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8"
                          onClick={() =>
                            handleAction(job.id, () => onTrigger(job.id))
                          }
                          disabled={actionLoading === job.id}
                        >
                          <PlayIcon className="h-4 w-4" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Trigger now</TooltipContent>
                    </Tooltip>
                    <Switch
                      checked={job.enabled}
                      aria-label={job.enabled ? `Disable ${job.name}` : `Enable ${job.name}`}
                      onCheckedChange={() =>
                        handleAction(
                          job.id,
                          job.enabled
                            ? () => onDisable(job.id)
                            : () => onEnable(job.id)
                        )
                      }
                      disabled={actionLoading === job.id}
                    />
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8"
                          asChild
                        >
                          <Link href={`/jobs/${job.id}/edit`}>
                            <PencilIcon className="h-4 w-4" />
                          </Link>
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Edit</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant="ghost"
                          size="icon"
                          className="h-8 w-8"
                          onClick={() => setDeleteTarget(job)}
                          disabled={actionLoading === job.id}
                        >
                          <TrashIcon className="h-4 w-4" />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Delete</TooltipContent>
                    </Tooltip>
                  </div>
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </div>

      {/* Delete Confirmation */}
      <AlertDialog
        open={!!deleteTarget}
        onOpenChange={(open) => !open && setDeleteTarget(null)}
      >
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Delete Job</AlertDialogTitle>
            <AlertDialogDescription>
              Are you sure you want to delete{" "}
              <strong>{deleteTarget?.name}</strong>? This action cannot be
              undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              onClick={handleDelete}
              disabled={actionLoading !== null}
              className="bg-destructive text-destructive-foreground hover:bg-destructive/90"
            >
              {actionLoading ? "Deleting..." : "Delete"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </>
  );
}
