"use client";

import React from "react";
import { ClockIcon, ServerIcon, HashtagIcon, FolderOpenIcon } from "@heroicons/react/24/outline";
import { CheckCircleIcon } from "@heroicons/react/20/solid";
import { HealthCard } from "./HealthCard";
import { Badge } from "@/components/ui/badge";
import type { HealthResponse } from "@/lib/types";
import { formatUptime } from "@/lib/format";

interface DashboardGridProps {
  health: HealthResponse;
}

export function DashboardGrid({ health }: DashboardGridProps) {
  return (
    <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4 w-full">
      <HealthCard
        label="Status"
        value={
          <Badge variant={health.status === "ok" ? "success" : "error"}>
            {health.status}
          </Badge>
        }
        icon={<CheckCircleIcon className="h-5 w-5" />}
      />
      <HealthCard
        label="Uptime"
        value={formatUptime(health.uptime_seconds)}
        icon={<ClockIcon className="h-5 w-5" />}
      />
      <HealthCard
        label="Active / Total Jobs"
        value={`${health.active_jobs} / ${health.total_jobs}`}
        icon={<ServerIcon className="h-5 w-5" />}
      />
      <HealthCard
        label="Version"
        value={health.version}
        icon={<HashtagIcon className="h-5 w-5" />}
      />
      <HealthCard
        label="Data Directory"
        value={health.data_dir}
        tooltip={health.data_dir}
        icon={<FolderOpenIcon className="h-5 w-5" />}
      />
    </div>
  );
}
