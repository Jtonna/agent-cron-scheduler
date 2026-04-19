"use client";

import { useRef, useCallback } from "react";
import { useHealth } from "@/hooks/useHealth";
import { useGlobalCostSummary } from "@/hooks/useGlobalCostSummary";
import { useJobs } from "@/hooks/useJobs";
import { useSSEEvents } from "@/hooks/useSSE";
import { HeroSection } from "@/components/dashboard/HeroSection";
import { RecentRuns } from "@/components/dashboard/RecentRuns";
import { SystemLogs } from "@/components/dashboard/SystemLogs";
import { Footer } from "@/components/dashboard/Footer";

export function MonitoringPage() {
  const { health, refresh: healthRefresh } = useHealth(5000);
  const { summary, refresh: summaryRefresh } = useGlobalCostSummary();
  const { jobs, refresh: jobsRefresh } = useJobs();

  // SSE event wiring: refresh data on relevant events
  useSSEEvents(
    useCallback(
      (event) => {
        if (
          event.type === "job_changed" ||
          event.type === "completed" ||
          event.type === "failed" ||
          event.type === "killed"
        ) {
          healthRefresh();
          summaryRefresh();
          jobsRefresh();
        }
      },
      [healthRefresh, summaryRefresh, jobsRefresh]
    )
  );

  // Scroll-to-logs ref
  const logsRef = useRef<HTMLDivElement>(null);
  const scrollToLogs = useCallback(() => {
    logsRef.current?.scrollIntoView({ behavior: "smooth" });
  }, []);

  return (
    <div className="flex flex-col min-h-full">
      <div className="flex-1 mx-auto w-full max-w-6xl px-6 py-6 space-y-6 pb-12">
        <HeroSection summary={summary} jobs={jobs} scrollToLogs={scrollToLogs} />
        <RecentRuns />
        <div ref={logsRef}>
          <SystemLogs />
        </div>
      </div>
      <Footer health={health} />
    </div>
  );
}
