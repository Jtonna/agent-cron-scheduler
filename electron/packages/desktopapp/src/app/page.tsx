"use client";

import { useState, useEffect, useRef } from "react";
import { useRouter } from "next/navigation";
import { Navbar } from "@/components/navbar/Navbar";
import { FilterTabs } from "@/components/jobs/FilterTabs";
import { ChatBar } from "@/components/ui/ChatBar";
import { FavoritedJobs } from "@/components/jobs/FavoritedJobs";
import { SystemBanner } from "@/components/widgets/SystemBanner";
import { TabBar } from "@/components/ui/TabBar";
import { JobRunCard } from "@/components/jobs/JobRunCard";
import type { JobRun } from "@/components/jobs/JobRunCard";
import { apiStatusToJobState } from "@/components/ui/JobStateIndicator";
import { useHealth } from "@/apis/useHealth";
import { useJobs } from "@/apis/useJobs";
import { useRecentRuns } from "@/apis/useRecentRuns";
import type { RecentRunEntry } from "@/apis/types";
import { formatDuration, formatTimeAgo, formatUptime } from "@/apis/format";
import { Loader2 } from "lucide-react";

const STATUS_FILTER_MAP: Record<string, string | undefined> = {
  "All runs": undefined,
  Running: "running",
  Succeeded: "success",
  Failed: "failed",
};

/* ------------------------------------------------------------------ */
/*  Helpers                                                           */
/* ------------------------------------------------------------------ */

function toJobRun(entry: RecentRunEntry): JobRun {
  return {
    name: entry.workflow_snapshot.name,
    status: apiStatusToJobState(entry.status),
    duration: formatDuration(entry.total_duration_ms),
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
  const { jobs } = useJobs();
  const { runs, loading: runsLoading, loadingMore, hasMore, loadMore } = useRecentRuns();

  // Real favorites from the workflow store. Sort alphabetically for a
  // stable order across re-fetches.
  const favoritedJobs = jobs
    .filter((j) => j.is_favorited)
    .map((j) => ({ id: j.id, name: j.name }))
    .sort((a, b) => a.name.localeCompare(b.name));
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

  // Cheap derivations over small arrays — no useMemo needed.
  const jobRuns = runs.map(toJobRun);
  const statusFilter = STATUS_FILTER_MAP[activeFilter];
  const filteredRuns = statusFilter ? jobRuns.filter((job) => job.status === statusFilter) : jobRuns;

  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />

      {/* Hero */}
      <section className="px-16 pt-14 pb-10 grid grid-cols-[1fr_1fr] gap-8 items-center">
        <div className="flex flex-col gap-5 max-w-xl">
          <h1 className="text-[52px] font-extrabold leading-[1.05] tracking-tight">
            Welcome to
            <br />
            Agent Cron System
          </h1>
          <p className="text-fg-muted text-[15px] leading-relaxed max-w-md">
            Schedule and run AI agent jobs on demand or on a cron. Build automations, manage tasks,
            and orchestrate your infrastructure from one place.
          </p>
          <FilterTabs />
          <ChatBar onSend={(msg) => router.push(`/chat?q=${encodeURIComponent(msg)}`)} />
          <FavoritedJobs jobs={favoritedJobs} />
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
      <div className="px-16">
        <TabBar
          label="Recent"
          tabs={["All runs", "Running", "Succeeded", "Failed"]}
          activeTab={activeFilter}
          onTabClick={setActiveFilter}
        />
      </div>
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
          filteredRuns.map((job, i) => <JobRunCard key={i} job={job} />)
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
