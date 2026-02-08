"use client";

import React from "react";
import { Grid } from "@once-ui-system/core";
import { HealthCard } from "./HealthCard";
import type { HealthResponse } from "@/lib/types";
import { formatUptime } from "@/lib/format";

interface DashboardGridProps {
  health: HealthResponse;
}

export function DashboardGrid({ health }: DashboardGridProps) {
  return (
    <Grid columns="3" gap="16" fillWidth>
      <HealthCard
        label="Status"
        value={health.status}
        icon={
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="9" cy="9" r="7" />
            <path d="M6 9l2 2 4-4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        }
      />
      <HealthCard
        label="Uptime"
        value={formatUptime(health.uptime_seconds)}
        icon={
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5">
            <circle cx="9" cy="9" r="7" />
            <path d="M9 5v4l2.5 2.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        }
      />
      <HealthCard
        label="Active / Total Jobs"
        value={`${health.active_jobs} / ${health.total_jobs}`}
        icon={
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5">
            <rect x="2" y="4" width="14" height="10" rx="1.5" />
            <path d="M5 8h8M5 11h5" strokeLinecap="round" />
          </svg>
        }
      />
      <HealthCard
        label="Version"
        value={health.version}
        icon={
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M5 3v12M9 3v12M13 3v12" strokeLinecap="round" />
          </svg>
        }
      />
      <HealthCard
        label="Data Directory"
        value={health.data_dir}
        icon={
          <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" strokeWidth="1.5">
            <path d="M2 5.5l2-2h4l1 1h5a1.5 1.5 0 011.5 1.5v7a1.5 1.5 0 01-1.5 1.5H3.5A1.5 1.5 0 012 12V5.5z" />
          </svg>
        }
      />
    </Grid>
  );
}
