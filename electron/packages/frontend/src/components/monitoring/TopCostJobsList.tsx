"use client";

import React from "react";
import { useNavigate } from "react-router-dom";
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@/components/ui/table";
import { formatCost } from "@/lib/format";
import type { GlobalTopJob } from "@/lib/types";

interface TopCostJobsListProps {
  topJobs: GlobalTopJob[];
}

/**
 * Renders the top-5 cost jobs as a clickable table.
 * Displays job name, total cost, total runs, and average cost per run.
 * Rows navigate to the job detail page on click.
 */
export function TopCostJobsList({ topJobs }: TopCostJobsListProps) {
  const navigate = useNavigate();

  if (topJobs.length === 0) {
    return (
      <div className="flex items-center justify-center rounded-lg border border-dashed border-muted-foreground/25 p-8 text-center">
        <p className="text-sm text-muted-foreground">No cost data yet</p>
      </div>
    );
  }

  return (
    <Table>
      <TableHeader>
        <TableRow>
          <TableHead>Job Name</TableHead>
          <TableHead className="text-right">Total Cost</TableHead>
          <TableHead className="text-right">Runs</TableHead>
          <TableHead className="text-right">Avg Cost/Run</TableHead>
        </TableRow>
      </TableHeader>
      <TableBody>
        {topJobs.map((job) => {
          const avgCost =
            job.total_runs > 0 ? job.total_cost / job.total_runs : 0;
          return (
            <TableRow
              key={job.job_id}
              onClick={() => navigate(`/jobs/${job.job_id}`)}
              className="cursor-pointer"
            >
              <TableCell className="font-medium">{job.job_name}</TableCell>
              <TableCell className="text-right">
                {formatCost(job.total_cost)}
              </TableCell>
              <TableCell className="text-right">{job.total_runs}</TableCell>
              <TableCell className="text-right">{formatCost(avgCost)}</TableCell>
            </TableRow>
          );
        })}
      </TableBody>
    </Table>
  );
}
