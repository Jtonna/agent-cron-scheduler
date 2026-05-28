"use client";

import { use, useState } from "react";
import { Loader2 } from "lucide-react";
import { Navbar } from "@/components/navbar/Navbar";
import { LogViewer } from "@/components/ui/LogViewer";
import { RunHeader } from "@/components/jobs/RunHeader";
import { RunStats } from "@/components/jobs/RunStats";
import { KillRunButton } from "@/components/jobs/KillRunButton";
import { RunDetailSidebar } from "@/components/sidebar/RunDetailSidebar";
import { useRun } from "@/apis/useRun";
import { useRunLog } from "@/apis/useRunLog";
import { useJob } from "@/apis/useJob";

/**
 * Run detail page (`/workflows/[id]/runs/[runId]`).
 *
 * Composes the run-detail experience: a sticky sidebar (back link, workflow
 * identity, step list), a thin header strip, a row of stat tiles, and a
 * bounded-height log viewer that live-tails via SSE. When the user picks a
 * step in the sidebar, the log viewer scrolls to that step's starting line.
 */
export default function RunDetailPage({
  params,
}: {
  params: Promise<{ id: string; runId: string }>;
}) {
  const { id, runId } = use(params);

  const { run, loading: runLoading, error: runError } = useRun(runId);
  const { logs } = useRunLog(runId);
  const { job } = useJob(id);

  const [activeStepIndex, setActiveStepIndex] = useState<number | null>(null);

  // Translate the active step's byte offset into a 1-based line number for
  // LazyLog. We treat byte offset as character offset — this is exact for
  // ASCII and a close approximation for typical log content (the worst case
  // is a few-line drift when steps emit non-ASCII).
  let scrollToLine: number | undefined = undefined;
  if (run && activeStepIndex !== null && run.steps[activeStepIndex]) {
    const offset = run.steps[activeStepIndex].log_byte_offset_start;
    if (offset > 0 && logs.length > 0) {
      scrollToLine = logs.slice(0, offset).split("\n").length;
    } else {
      scrollToLine = 1;
    }
  }

  const workflowName = run?.workflow_snapshot.name ?? job?.name ?? "";
  const cron = job?.schedule ?? "";
  const totalSteps = job?.steps?.length ?? null;

  return (
    <div className="min-h-screen bg-surface text-fg">
      <Navbar />

      <div className="grid grid-cols-[var(--width-sidebar)_1fr]">
        {/* Sidebar */}
        <div className="sticky top-[var(--height-navbar)] h-[calc(100vh-var(--height-navbar))] border-r border-border-subtle">
          {run ? (
            <RunDetailSidebar
              jobId={id}
              workflowName={workflowName}
              cron={cron}
              runSteps={run.steps}
              totalSteps={totalSteps}
              activeStepIndex={activeStepIndex}
              onSelectStep={setActiveStepIndex}
            />
          ) : (
            <div className="p-6 flex items-center justify-center text-fg-subtle text-sm">
              {runLoading ? (
                <Loader2 size={20} className="animate-spin" />
              ) : (
                "Run not found"
              )}
            </div>
          )}
        </div>

        {/* Main content */}
        <main className="px-8 py-8 flex flex-col gap-6">
          {runError ? (
            <div className="rounded-card border border-status-failed-border bg-status-failed-bg p-4 text-status-failed text-sm">
              Failed to load run: {runError}
            </div>
          ) : !run && runLoading ? (
            <div className="flex items-center justify-center py-24">
              <Loader2 size={24} className="animate-spin text-fg-subtle" />
            </div>
          ) : !run ? (
            <div className="text-fg-muted text-sm">
              Run <span className="font-mono">{runId}</span> doesn&apos;t exist or has been deleted.
            </div>
          ) : (
            <>
              <div className="flex flex-col gap-2">
                <RunHeader
                  workflowName={workflowName}
                  runId={run.run_id}
                  status={run.status}
                />
                <RunStats run={run} />
              </div>

              <div className="h-[600px] rounded-input border border-border overflow-hidden">
                <LogViewer
                  text={logs}
                  scrollToLine={scrollToLine}
                  actions={
                    run.status === "Running" ? (
                      <KillRunButton runId={runId} size="sm" />
                    ) : null
                  }
                />
              </div>
            </>
          )}
        </main>
      </div>
    </div>
  );
}
