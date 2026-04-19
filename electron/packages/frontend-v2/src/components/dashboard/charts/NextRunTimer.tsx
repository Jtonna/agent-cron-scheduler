"use client";

import { useEffect, useState } from "react";
import { ClockIcon } from "@heroicons/react/24/outline";
import type { Job } from "@/lib/types";

interface NextRunTimerProps {
  jobs: Job[];
}

interface NextRun {
  jobName: string;
  schedule: string;
  targetTime: Date;
}

function findNextRun(jobs: Job[]): NextRun | null {
  const now = new Date();
  let soonest: NextRun | null = null;

  for (const job of jobs) {
    if (!job.next_run_at || !job.enabled) continue;
    const target = new Date(job.next_run_at);
    if (target <= now) continue;

    if (!soonest || target < soonest.targetTime) {
      soonest = {
        jobName: job.name,
        schedule: job.schedule,
        targetTime: target,
      };
    }
  }

  return soonest;
}

function formatCountdown(ms: number): string {
  if (ms <= 0) return "00:00:00";

  const totalSecs = Math.floor(ms / 1000);
  const hours = Math.floor(totalSecs / 3600);
  const minutes = Math.floor((totalSecs % 3600) / 60);
  const seconds = totalSecs % 60;

  const pad = (n: number) => n.toString().padStart(2, "0");
  return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}`;
}

export function NextRunTimer({ jobs }: NextRunTimerProps) {
  const [nextRun, setNextRun] = useState<NextRun | null>(() => findNextRun(jobs));
  const [remaining, setRemaining] = useState<number>(0);

  // Re-evaluate which job is next when the jobs list changes
  useEffect(() => {
    const run = findNextRun(jobs);
    setNextRun(run);
    if (run) {
      setRemaining(run.targetTime.getTime() - Date.now());
    }
  }, [jobs]);

  // Countdown interval
  useEffect(() => {
    if (!nextRun) return;

    const tick = () => {
      const diff = nextRun.targetTime.getTime() - Date.now();
      setRemaining(Math.max(diff, 0));

      // If countdown reached zero, re-evaluate
      if (diff <= 0) {
        const run = findNextRun(jobs);
        setNextRun(run);
      }
    };

    tick();
    const id = setInterval(tick, 1000);
    return () => clearInterval(id);
  }, [nextRun, jobs]);

  if (!nextRun) {
    return (
      <div className="flex h-full flex-col items-center justify-center gap-2 text-muted-foreground">
        <ClockIcon className="size-8" />
        <span className="text-sm">No scheduled runs</span>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col items-center justify-center gap-3">
      <ClockIcon className="size-8 text-primary" />
      <div className="text-center">
        <p className="text-sm font-medium text-foreground truncate max-w-[200px]">
          {nextRun.jobName}
        </p>
        <p className="mt-1 font-mono text-2xl font-bold tabular-nums text-foreground">
          {formatCountdown(remaining)}
        </p>
        <p className="mt-1 text-[11px] text-muted-foreground font-mono">
          {nextRun.schedule}
        </p>
      </div>
    </div>
  );
}
