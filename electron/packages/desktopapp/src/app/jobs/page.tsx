"use client";

import { Navbar } from "@/components/navbar/Navbar";
import { JobsSidebar } from "@/components/sidebar/JobsSidebar";
import { JobsList } from "@/components/jobs/JobsList";
import { CostWidget } from "@/components/widgets/CostWidget";
import { TokensWidget } from "@/components/widgets/TokensWidget";
import { HealthWidget } from "@/components/widgets/HealthWidget";
import { TopSpendersWidget } from "@/components/widgets/TopSpendersWidget";
import { CostTrendWidget } from "@/components/widgets/CostTrendWidget";
import { useJobs } from "@/apis/useJobs";
import { useGlobalCostSummary } from "@/apis/useGlobalCostSummary";
import { useRecentRuns } from "@/apis/useRecentRuns";

export default function JobsPage() {
  const { jobs, loading: jobsLoading } = useJobs();
  const { summary, loading: summaryLoading } = useGlobalCostSummary();
  const { runs } = useRecentRuns(200);

  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />

      <div className="grid grid-cols-[var(--width-sidebar)_1fr]">
        {/* Sidebar */}
        <div className="sticky top-[var(--height-navbar)] h-[calc(100vh-var(--height-navbar))] overflow-y-auto border-r border-border-subtle">
          <JobsSidebar jobs={jobs} />
        </div>

        {/* Main content */}
        <main className="px-8 py-8 flex flex-col gap-6">
          <div>
            <h1 className="text-2xl font-extrabold tracking-tight">Jobs</h1>
            <p className="text-fg-muted text-sm mt-0.5">Manage and inspect every scheduled job.</p>
          </div>

          {/* Widgets row */}
          <div className="grid grid-cols-4 gap-4">
            <CostWidget summary={summary?.system_cost_summary ?? null} loading={summaryLoading} />
            <TokensWidget summary={summary?.system_cost_summary ?? null} loading={summaryLoading} />
            <HealthWidget runs={runs} />
            <TopSpendersWidget workflows={summary?.workflows ?? []} />
          </div>

          {/* Cost trend */}
          <CostTrendWidget data={summary?.system_cost_summary?.daily_buckets ?? []} />

          {/* Jobs list */}
          <JobsList jobs={jobs} loading={jobsLoading} />
        </main>
      </div>
    </div>
  );
}
