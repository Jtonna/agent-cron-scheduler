"use client";

import React from "react";
import { usePathname } from "next/navigation";
import { Column, Text, ToggleButton } from "@once-ui-system/core";
import type { Job } from "@/lib/types";

interface RecentJobsWidgetProps {
  jobs: Job[];
}

export function RecentJobsWidget({ jobs }: RecentJobsWidgetProps) {
  const pathname = usePathname();

  const recentJobs = jobs
    .filter((j) => j.last_run_at)
    .sort(
      (a, b) =>
        new Date(b.last_run_at!).getTime() -
        new Date(a.last_run_at!).getTime()
    )
    .slice(0, 7);

  if (recentJobs.length === 0) {
    return (
      <Column paddingX="12" paddingY="8">
        <Text variant="body-default-xs" onBackground="neutral-weak">
          No recent runs
        </Text>
      </Column>
    );
  }

  return (
    <Column gap="2">
      {recentJobs.map((job) => (
        <ToggleButton
          key={job.id}
          fillWidth
          horizontal="start"
          prefixIcon={job.last_exit_code === 0 ? "checkCircle" : "xCircle"}
          label={job.name}
          selected={pathname === `/jobs/${job.id}`}
          onClick={() => { window.location.href = `/jobs/${job.id}`; }}
        />
      ))}
    </Column>
  );
}
