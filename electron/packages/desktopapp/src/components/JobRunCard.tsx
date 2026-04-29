"use client";

import { useState } from "react";
import { Button } from "react-aria-components";
import { Clock, DollarSign, Loader2, CheckCircle2, XCircle, AlertTriangle, ArrowRight, Ban } from "lucide-react";

export type JobStatus = "running" | "success" | "failed" | "killed" | "partial";

export interface JobRun {
  name: string;
  status: JobStatus;
  duration: string;
  timeAgo: string;
  cost?: string;
}

const statusConfig: Record<JobStatus, { icon: React.ReactNode; label: string; color: string; dotColor: string; badgeBg: string; badgeBorder: string }> = {
  running: {
    icon: <Loader2 size={14} className="animate-spin" />,
    label: "Running",
    color: "text-status-running",
    dotColor: "bg-status-running-dot",
    badgeBg: "bg-status-running-bg",
    badgeBorder: "border-status-running-border",
  },
  success: {
    icon: <CheckCircle2 size={14} />,
    label: "Success",
    color: "text-status-success",
    dotColor: "bg-status-success-dot",
    badgeBg: "bg-status-success-bg",
    badgeBorder: "border-status-success-border",
  },
  failed: {
    icon: <XCircle size={14} />,
    label: "Failed",
    color: "text-status-failed",
    dotColor: "bg-status-failed-dot",
    badgeBg: "bg-status-failed-bg",
    badgeBorder: "border-status-failed-border",
  },
  killed: {
    icon: <Ban size={14} />,
    label: "Killed",
    color: "text-status-killed",
    dotColor: "bg-status-killed-dot",
    badgeBg: "bg-status-killed-bg",
    badgeBorder: "border-status-killed-border",
  },
  partial: {
    icon: <AlertTriangle size={14} />,
    label: "Partial",
    color: "text-status-partial",
    dotColor: "bg-status-partial-dot",
    badgeBg: "bg-status-partial-bg",
    badgeBorder: "border-status-partial-border",
  },
};

interface JobRunCardProps {
  job: JobRun;
  onClick?: (job: JobRun) => void;
}

export function JobRunCard({ job, onClick }: JobRunCardProps) {
  const [hovered, setHovered] = useState(false);
  const cfg = statusConfig[job.status];

  return (
    <Button
      onPress={() => onClick?.(job)}
      onHoverStart={() => setHovered(true)}
      onHoverEnd={() => setHovered(false)}
      className="relative overflow-hidden border border-border rounded-card bg-surface cursor-pointer hover:border-brand-ring transition-colors text-left outline-none focus-visible:ring-2 focus-visible:ring-brand-ring w-full"
      aria-label={`${job.name} — ${cfg.label}`}
    >
      <div className={`p-4 transition-transform duration-200 ease-out ${hovered ? "-translate-x-12" : "translate-x-0"}`}>
        {/* Top row — name + cost */}
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <span className={`w-[var(--size-status-dot)] h-[var(--size-status-dot)] rounded-full shrink-0 ${cfg.dotColor} ${job.status === "running" ? "animate-pulse" : ""}`} />
            <span className="text-sm font-semibold text-fg truncate">{job.name}</span>
          </div>
          {job.cost && (
            <span className="inline-flex items-center gap-1 text-xs text-fg-subtle font-mono">
              <DollarSign size={11} />
              {job.cost}
            </span>
          )}
        </div>
        {/* Bottom row — duration, time, status badge */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3 text-xs text-fg-subtle">
            <span className="inline-flex items-center gap-1">
              <Clock size={12} />
              {job.duration}
            </span>
            <span>{job.timeAgo}</span>
          </div>
          <span className={`inline-flex items-center gap-1 text-xs font-medium px-2 py-0.5 rounded-badge border ${cfg.color} ${cfg.badgeBg} ${cfg.badgeBorder}`}>
            {cfg.icon}
            {cfg.label}
          </span>
        </div>
      </div>

      {/* Slide-in panel from right */}
      <div
        className={`absolute inset-y-0 right-0 w-12 flex items-center justify-center bg-brand text-surface transition-transform duration-200 ease-out ${
          hovered ? "translate-x-0" : "translate-x-full"
        }`}
        aria-hidden
      >
        <ArrowRight size={16} />
      </div>
    </Button>
  );
}
