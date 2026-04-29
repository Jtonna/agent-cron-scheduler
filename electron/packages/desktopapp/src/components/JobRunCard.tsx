"use client";

import { useState } from "react";
import { Clock, DollarSign, Loader2, CheckCircle2, XCircle, AlertTriangle, ArrowRight } from "lucide-react";

export type JobStatus = "running" | "success" | "failed" | "partial";

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
    color: "text-blue-600",
    dotColor: "bg-blue-500",
    badgeBg: "bg-blue-50",
    badgeBorder: "border-blue-200",
  },
  success: {
    icon: <CheckCircle2 size={14} />,
    label: "Success",
    color: "text-green-600",
    dotColor: "bg-green-500",
    badgeBg: "bg-green-50",
    badgeBorder: "border-green-200",
  },
  failed: {
    icon: <XCircle size={14} />,
    label: "Failed",
    color: "text-red-500",
    dotColor: "bg-red-500",
    badgeBg: "bg-red-50",
    badgeBorder: "border-red-200",
  },
  partial: {
    icon: <AlertTriangle size={14} />,
    label: "Partial",
    color: "text-amber-500",
    dotColor: "bg-amber-400",
    badgeBg: "bg-amber-50",
    badgeBorder: "border-amber-200",
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
    <div
      className="relative overflow-hidden border border-gray-200 rounded-xl bg-white cursor-pointer hover:border-pink-300 transition-colors"
      onMouseEnter={() => setHovered(true)}
      onMouseLeave={() => setHovered(false)}
      onClick={() => onClick?.(job)}
    >
      <div className={`p-4 transition-transform duration-200 ease-out ${hovered ? "-translate-x-12" : "translate-x-0"}`}>
        {/* Top row — name + cost */}
        <div className="flex items-center justify-between mb-3">
          <div className="flex items-center gap-2">
            <span className={`w-2 h-2 rounded-full shrink-0 ${cfg.dotColor} ${job.status === "running" ? "animate-pulse" : ""}`} />
            <span className="text-sm font-semibold text-gray-900 truncate">{job.name}</span>
          </div>
          {job.cost && (
            <span className="inline-flex items-center gap-1 text-xs text-gray-400 font-mono">
              <DollarSign size={11} />
              {job.cost}
            </span>
          )}
        </div>
        {/* Bottom row — duration, time, status badge */}
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3 text-xs text-gray-400">
            <span className="inline-flex items-center gap-1">
              <Clock size={12} />
              {job.duration}
            </span>
            <span>{job.timeAgo}</span>
          </div>
          <span className={`inline-flex items-center gap-1 text-xs font-medium px-2 py-0.5 rounded-full border ${cfg.color} ${cfg.badgeBg} ${cfg.badgeBorder}`}>
            {cfg.icon}
            {cfg.label}
          </span>
        </div>
      </div>

      {/* Slide-in panel from right */}
      <div
        className={`absolute inset-y-0 right-0 w-12 flex items-center justify-center bg-pink-500 text-white transition-transform duration-200 ease-out ${
          hovered ? "translate-x-0" : "translate-x-full"
        }`}
      >
        <ArrowRight size={16} />
      </div>
    </div>
  );
}
