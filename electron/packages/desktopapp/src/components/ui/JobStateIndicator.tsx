"use client";

import { Loader2, CheckCircle2, XCircle, Ban, AlertTriangle, type LucideIcon } from "lucide-react";
import type { JobRun } from "@/apis/types";

/**
 * JobStateIndicator
 *
 * Single source of truth for "this job/run is in state X" visuals. Used by
 * every component that needs to communicate run/job state.
 *
 * Variants: `dot` (colored circle), `badge` (pill with icon + label), `label`
 * (just colored text). Sizes only affect the dot variant. The `running`
 * state auto-pulses; pass `pulse={false}` to disable, or `pulse` on a
 * non-running state to opt in.
 */

export type JobState = "running" | "success" | "failed" | "killed" | "warning" | "idle";

export type JobStateVariant = "dot" | "badge" | "label";
export type JobStateSize = "xs" | "sm" | "md" | "lg";

interface JobStateIndicatorProps {
  state: JobState;
  variant?: JobStateVariant;
  size?: JobStateSize;
  /** Override the auto-pulse behavior (running auto-pulses by default). */
  pulse?: boolean;
  className?: string;
  /** Used as aria-label on dots and as the visible text on badges/labels. */
  label?: string;
}

interface StateConfig {
  dotClass: string;
  textClass: string;
  badgeBg: string;
  badgeBorder: string;
  icon: LucideIcon | null;
  label: string;
}

const STATE_CONFIG: Record<JobState, StateConfig> = {
  running: {
    dotClass: "bg-status-running-dot",
    textClass: "text-status-running",
    badgeBg: "bg-status-running-bg",
    badgeBorder: "border-status-running-border",
    icon: Loader2,
    label: "Running",
  },
  success: {
    dotClass: "bg-status-success-dot",
    textClass: "text-status-success",
    badgeBg: "bg-status-success-bg",
    badgeBorder: "border-status-success-border",
    icon: CheckCircle2,
    label: "Succeeded",
  },
  failed: {
    dotClass: "bg-status-failed-dot",
    textClass: "text-status-failed",
    badgeBg: "bg-status-failed-bg",
    badgeBorder: "border-status-failed-border",
    icon: XCircle,
    label: "Failed",
  },
  killed: {
    dotClass: "bg-status-killed-dot",
    textClass: "text-status-killed",
    badgeBg: "bg-status-killed-bg",
    badgeBorder: "border-status-killed-border",
    icon: Ban,
    label: "Killed",
  },
  warning: {
    dotClass: "bg-status-warning-dot",
    textClass: "text-status-warning",
    badgeBg: "bg-status-warning-bg",
    badgeBorder: "border-status-warning-border",
    icon: AlertTriangle,
    label: "Warning",
  },
  idle: {
    dotClass: "bg-fg-faint",
    textClass: "text-fg-muted",
    badgeBg: "bg-surface-secondary",
    badgeBorder: "border-border-subtle",
    icon: null,
    label: "Idle",
  },
};

const SIZE_CLASSES: Record<JobStateSize, string> = {
  xs: "w-1.5 h-1.5", // 6px
  sm: "w-2 h-2", // 8px
  md: "w-2.5 h-2.5", // 10px
  lg: "w-3 h-3", // 12px
};

/** Map an API run status to the corresponding JobState. */
export function apiStatusToJobState(status: JobRun["status"]): JobState {
  switch (status) {
    case "Running":
      return "running";
    case "Completed":
      return "success";
    case "Failed":
      return "failed";
    case "Killed":
      return "killed";
    case "CompletedWithWarnings":
      return "warning";
  }
}

export function JobStateIndicator({
  state,
  variant = "dot",
  size = "sm",
  pulse,
  className = "",
  label,
}: JobStateIndicatorProps) {
  const cfg = STATE_CONFIG[state];
  const visibleLabel = label ?? cfg.label;
  const shouldPulse = pulse ?? state === "running";

  if (variant === "label") {
    return <span className={`font-semibold ${cfg.textClass} ${className}`}>{visibleLabel}</span>;
  }

  if (variant === "badge") {
    const Icon = cfg.icon;
    return (
      <span
        className={`inline-flex items-center gap-1 text-xs font-medium px-2 py-0.5 rounded-badge border ${cfg.textClass} ${cfg.badgeBg} ${cfg.badgeBorder} ${className}`}
      >
        {Icon && (
          <Icon size={14} className={state === "running" ? "animate-spin" : ""} aria-hidden />
        )}
        {visibleLabel}
      </span>
    );
  }

  // dot variant
  return (
    <span
      className={`inline-block rounded-full shrink-0 ${SIZE_CLASSES[size]} ${cfg.dotClass} ${shouldPulse ? "animate-pulse" : ""} ${className}`}
      aria-label={visibleLabel}
    />
  );
}
