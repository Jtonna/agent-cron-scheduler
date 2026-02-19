"use client";

import React from "react";
import { usePathname, useRouter } from "next/navigation";
import {
  CheckCircleIcon as CheckCircleSolid,
  XCircleIcon as XCircleSolid,
} from "@heroicons/react/20/solid";
import { ArrowPathIcon } from "@heroicons/react/24/outline";
import {
  SidebarMenuSub,
  SidebarMenuSubItem,
  SidebarMenuSubButton,
} from "@/components/ui/sidebar";
import type { Job } from "@/lib/types";

interface RecentJobsWidgetProps {
  jobs: Job[];
}

/**
 * Renders recent job runs as SidebarMenuSub items.
 * Designed to slot into a parent Collapsible > SidebarMenuItem structure.
 */
export function RecentJobsWidget({ jobs }: RecentJobsWidgetProps) {
  const pathname = usePathname();
  const router = useRouter();

  const recentJobs = jobs
    .filter((j) => j.last_run_at)
    .sort(
      (a, b) =>
        new Date(b.last_run_at!).getTime() -
        new Date(a.last_run_at!).getTime()
    )
    .slice(0, 7);

  return (
    <SidebarMenuSub>
      {recentJobs.length === 0 ? (
        <SidebarMenuSubItem>
          <SidebarMenuSubButton className="pointer-events-none text-sidebar-foreground/50">
            <span className="text-xs">No recent runs</span>
          </SidebarMenuSubButton>
        </SidebarMenuSubItem>
      ) : (
        recentJobs.map((job) => (
          <SidebarMenuSubItem key={job.id}>
            <SidebarMenuSubButton
              isActive={pathname === `/jobs/${job.id}`}
              onClick={() => router.push(`/jobs/${job.id}`)}
            >
              {job.last_exit_code === null ? (
                <ArrowPathIcon className="size-4 text-muted-foreground shrink-0" />
              ) : job.last_exit_code === 0 ? (
                <CheckCircleSolid className="size-4 text-emerald-600 dark:text-emerald-400 shrink-0" />
              ) : (
                <XCircleSolid className="size-4 text-red-600 dark:text-red-400 shrink-0" />
              )}
              <span>{job.name}</span>
            </SidebarMenuSubButton>
          </SidebarMenuSubItem>
        ))
      )}
    </SidebarMenuSub>
  );
}
