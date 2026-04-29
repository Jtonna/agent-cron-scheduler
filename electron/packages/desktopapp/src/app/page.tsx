"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { Navbar } from "@/components/Navbar";
import { FilterTabs } from "@/components/FilterTabs";
import { ChatBar } from "@/components/ChatBar";
import { FavoritedJobs } from "@/components/FavoritedJobs";
import { SystemBanner } from "@/components/SystemBanner";
import { TabBar } from "@/components/TabBar";
import { JobRunCard, JobRun } from "@/components/JobRunCard";

const FAVORITED_JOBS = ["backup-db", "sync-users", "health-check", "deploy-staging", "cleanup-logs", "nightly-report"];

const RECENT_RUNS: JobRun[] = [
  { name: "backup-db", status: "running", duration: "0m 42s", timeAgo: "just now", cost: "0.003" },
  { name: "sync-users", status: "success", duration: "2m 14s", timeAgo: "2 min ago", cost: "0.042" },
  { name: "deploy-staging", status: "failed", duration: "1m 02s", timeAgo: "5 min ago", cost: "0.018" },
  { name: "health-check", status: "success", duration: "0m 08s", timeAgo: "3 min ago" },
  { name: "cleanup-logs", status: "partial", duration: "4m 31s", timeAgo: "8 min ago", cost: "0.091" },
  { name: "nightly-report", status: "success", duration: "12m 44s", timeAgo: "22 min ago", cost: "0.214" },
  { name: "index-rebuild", status: "failed", duration: "0m 03s", timeAgo: "30 min ago" },
  { name: "cert-renewal", status: "success", duration: "0m 22s", timeAgo: "1 hr ago" },
];

const STATUS_FILTER_MAP: Record<string, string | undefined> = {
  "All runs": undefined,
  "Running": "running",
  "Succeeded": "success",
  "Failed": "failed",
};

export default function Home() {
  const router = useRouter();
  const [activeFilter, setActiveFilter] = useState("All runs");

  return (
    <div className="min-h-screen bg-white text-gray-900">
      <Navbar />

      {/* Hero */}
      <section className="px-16 pt-14 pb-10 grid grid-cols-[1fr_1fr] gap-8 items-center">
        <div className="flex flex-col gap-5 max-w-xl">
          <h1 className="text-[52px] font-extrabold leading-[1.05] tracking-tight">
            Welcome to<br />Agent Cron System
          </h1>
          <p className="text-gray-500 text-[15px] leading-relaxed max-w-md">
            Schedule and run AI agent jobs on demand or on a cron. Build automations, manage tasks, and orchestrate your infrastructure from one place.
          </p>
          <FilterTabs />
          <ChatBar onSend={(msg) => router.push(`/chat?q=${encodeURIComponent(msg)}`)} />
          <FavoritedJobs jobs={FAVORITED_JOBS} />
        </div>
        <div className="bg-gradient-to-br from-pink-100 via-purple-50 to-sky-100 rounded-2xl min-h-[420px] flex items-center justify-center">
          <span className="text-gray-400 text-sm">Hero illustration</span>
        </div>
      </section>

      {/* System info */}
      <div className="px-16 mb-6">
        <SystemBanner updateAvailable="3.2.0" />
      </div>

      {/* Recent runs */}
      <TabBar
        label="Recent"
        tabs={["All runs", "Running", "Succeeded", "Failed"]}
        activeTab={activeFilter}
        onTabClick={setActiveFilter}
      />
      <div className="px-16 py-8 grid grid-cols-4 gap-4">
        {RECENT_RUNS
          .filter((job) => {
            const status = STATUS_FILTER_MAP[activeFilter];
            return !status || job.status === status;
          })
          .map((job, i) => (
            <JobRunCard key={i} job={job} />
          ))}
      </div>
    </div>
  );
}
