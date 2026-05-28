"use client";

import { use } from "react";
import { Loader2 } from "lucide-react";
import { Navbar } from "@/components/navbar/Navbar";
import { JobDetailSidebar } from "@/components/sidebar/JobDetailSidebar";
import { JobCostWidget } from "@/components/widgets/JobCostWidget";
import { HealthWidget } from "@/components/widgets/HealthWidget";
import { JobCostTrendWidget } from "@/components/widgets/JobCostTrendWidget";
import { JobRunsTable } from "@/components/jobs/JobRunsTable";
import { useJob } from "@/apis/useJob";
import { useJobCostSummary } from "@/apis/useJobCostSummary";
import { useJobRuns } from "@/apis/useJobRuns";

/**
 * Job detail page (`/workflows/[id]`).
 *
 * Layout mirrors `/workflows`: sticky JobDetailSidebar + main grid. Main shows
 * three widgets (Cost, Health, Cost trend) side-by-side, then a denser
 * runs table beneath. All action callbacks are non-functional placeholders
 * for now — the buttons render but don't talk to the backend yet.
 */
export default function JobDetailPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);

  const { job, loading: jobLoading, error: jobError } = useJob(id);
  const { summary, loading: summaryLoading } = useJobCostSummary(id);
  const { runs, loading: runsLoading } = useJobRuns(id, 100);

  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />

      <div className="grid grid-cols-[var(--width-sidebar)_1fr]">
        {/* Sidebar (manages its own internal scroll + pinned footer) */}
        <div className="sticky top-[var(--height-navbar)] h-[calc(100vh-var(--height-navbar))] border-r border-border-subtle">
          {job ? (
            <JobDetailSidebar job={job} />
          ) : (
            <div className="p-6 flex items-center justify-center text-fg-subtle text-sm">
              {jobLoading ? (
                <Loader2 size={20} className="animate-spin" />
              ) : (
                "Workflow not found"
              )}
            </div>
          )}
        </div>

        {/* Main content */}
        <main className="px-8 py-8 flex flex-col gap-6">
          {jobError ? (
            <div className="rounded-card border border-status-failed-border bg-status-failed-bg p-4 text-status-failed text-sm">
              Failed to load workflow: {jobError}
            </div>
          ) : !job && jobLoading ? (
            <div className="flex items-center justify-center py-24">
              <Loader2 size={24} className="animate-spin text-fg-subtle" />
            </div>
          ) : !job ? (
            <div className="text-fg-muted text-sm">
              Workflow <span className="font-mono">{id}</span> doesn&apos;t exist or has been deleted.
            </div>
          ) : (
            <>
              {/* Header — favorite control lives in the sidebar, not here. */}
              <div>
                <h1 className="text-2xl font-extrabold tracking-tight">{job.name}</h1>
                <p className="text-fg-muted text-sm mt-0.5 font-mono">{job.schedule}</p>
              </div>

              {/* 3-up widget row */}
              <div className="grid grid-cols-3 gap-4">
                <HealthWidget runs={runs} />
                <JobCostTrendWidget data={summary?.cost_summary?.daily_buckets ?? []} />
                <JobCostWidget summary={summary} loading={summaryLoading} />
              </div>

              {/* Runs table */}
              <JobRunsTable jobId={id} runs={runs} loading={runsLoading} />
            </>
          )}
        </main>
      </div>
    </div>
  );
}
