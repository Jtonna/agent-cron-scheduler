"use client";

import { useState, useMemo, useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import { Navbar } from "@/components/Navbar";
import { FilterTabs } from "@/components/FilterTabs";
import { ChatBar } from "@/components/ChatBar";
import { FavoritedJobs } from "@/components/FavoritedJobs";
import { SystemBanner } from "@/components/SystemBanner";
import { TabBar } from "@/components/TabBar";
import { JobRunCard } from "@/components/JobRunCard";
import type { JobRun, JobStatus } from "@/components/JobRunCard";
import { useHealth } from "@/apis/useHealth";
import { useRecentRuns } from "@/apis/useRecentRuns";
import type { RecentRunEntry } from "@/apis/types";
import { Loader2 } from "lucide-react";

const FAVORITED_JOBS = ["backup-db", "sync-users", "health-check", "deploy-staging", "cleanup-logs", "nightly-report"];

const STATUS_FILTER_MAP: Record<string, string | undefined> = {
  "All runs": undefined,
  "Running": "running",
  "Succeeded": "success",
  "Failed": "failed",
};

/* ------------------------------------------------------------------ */
/*  Helpers                                                           */
/* ------------------------------------------------------------------ */

function mapApiStatus(
  status: RecentRunEntry["status"],
): JobStatus {
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
      return "partial";
    default:
      return "failed";
  }
}

function formatDuration(ms: number): string {
  const totalSeconds = Math.floor(ms / 1000);
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${minutes}m ${String(seconds).padStart(2, "0")}s`;
}

function formatTimeAgo(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const seconds = Math.floor(diff / 1000);
  if (seconds < 60) return "just now";
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes} min ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours} hr ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function formatUptime(seconds: number): string {
  const d = Math.floor(seconds / 86400);
  const h = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${d}d ${h}h ${m}m`;
}

function toJobRun(entry: RecentRunEntry): JobRun {
  const durationMs =
    entry.duration_ms ??
    (entry.finished_at
      ? new Date(entry.finished_at).getTime() -
        new Date(entry.started_at).getTime()
      : Date.now() - new Date(entry.started_at).getTime());

  return {
    name: entry.job_name,
    status: mapApiStatus(entry.status),
    duration: formatDuration(durationMs),
    timeAgo: formatTimeAgo(entry.started_at),
    cost:
      entry.total_cost_usd != null && entry.total_cost_usd > 0
        ? entry.total_cost_usd.toFixed(3)
        : undefined,
  };
}

/* ------------------------------------------------------------------ */
/*  Page                                                              */
/* ------------------------------------------------------------------ */

export default function Home() {
  const router = useRouter();
  const [activeFilter, setActiveFilter] = useState("All runs");
  const { health } = useHealth();
  const { runs, loading: runsLoading, loadingMore, hasMore, loadMore } = useRecentRuns();
  const sentinelRef = useRef<HTMLDivElement>(null);

  // Infinite scroll: observe a sentinel at the bottom of the grid and call loadMore when visible
  useEffect(() => {
    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    const observer = new IntersectionObserver(
      (entries) => {
        if (entries[0].isIntersecting && hasMore && !runsLoading && !loadingMore) {
          loadMore();
        }
      },
      { rootMargin: "200px" },
    );
    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [hasMore, runsLoading, loadingMore, loadMore]);

  const jobRuns = useMemo(() => runs.map(toJobRun), [runs]);

  const filteredRuns = useMemo(() => {
    const status = STATUS_FILTER_MAP[activeFilter];
    if (!status) return jobRuns;
    return jobRuns.filter((job) => job.status === status);
  }, [jobRuns, activeFilter]);

  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />

      {/* Hero */}
      <section className="px-16 pt-14 pb-10 grid grid-cols-[1fr_1fr] gap-8 items-center">
        <div className="flex flex-col gap-5 max-w-xl">
          <h1 className="text-[52px] font-extrabold leading-[1.05] tracking-tight">
            Welcome to<br />Agent Cron System
          </h1>
          <p className="text-fg-muted text-[15px] leading-relaxed max-w-md">
            Schedule and run AI agent jobs on demand or on a cron. Build automations, manage tasks, and orchestrate your infrastructure from one place.
          </p>
          <FilterTabs />
          <ChatBar onSend={(msg) => router.push(`/chat?q=${encodeURIComponent(msg)}`)} />
          <FavoritedJobs jobs={FAVORITED_JOBS} />
        </div>
        <div className="bg-gradient-to-br from-gradient-hero-from via-gradient-hero-via to-gradient-hero-to rounded-card min-h-[420px] flex items-center justify-center">
          <span className="text-fg-subtle text-sm">Hero illustration</span>
        </div>
      </section>

      {/* System info */}
      <div className="px-16 mb-6">
        <SystemBanner
          version={health?.version}
          uptime={health ? formatUptime(health.uptime_seconds) : undefined}
        />
      </div>

      {/* Recent runs */}
      <TabBar
        label="Recent"
        tabs={["All runs", "Running", "Succeeded", "Failed"]}
        activeTab={activeFilter}
        onTabClick={setActiveFilter}
      />
      <div className="px-16 py-8 grid grid-cols-4 gap-4">
        {runsLoading && filteredRuns.length === 0 ? (
          <div className="col-span-4 flex items-center justify-center py-16">
            <Loader2 size={24} className="animate-spin text-fg-subtle" />
          </div>
        ) : filteredRuns.length === 0 ? (
          <div className="col-span-4 text-center py-16 text-fg-subtle text-sm">
            {activeFilter === "All runs"
              ? "No recent runs"
              : `No ${activeFilter.toLowerCase()} runs in the last ${runs.length}`}
          </div>
        ) : (
          filteredRuns.map((job, i) => (
            <JobRunCard key={i} job={job} />
          ))
        )}
      </div>

      {/* Infinite scroll sentinel + loading more indicator */}
      {filteredRuns.length > 0 && (
        <div ref={sentinelRef} className="px-16 pb-12 flex items-center justify-center">
          {loadingMore ? (
            <Loader2 size={20} className="animate-spin text-fg-subtle" />
          ) : !hasMore ? (
            <span className="text-fg-subtle text-xs">No more runs</span>
          ) : null}
        </div>
      )}
    </div>
  );
}
