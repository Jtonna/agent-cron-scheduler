"use client";

import { ChevronLeft } from "lucide-react";
import { formatDuration } from "@/apis/format";
import type { WorkflowRunStep } from "@/apis/types";
import { Button } from "@/components/ui/Button";
import {
  JobStateIndicator,
  apiStatusToJobState,
} from "@/components/ui/JobStateIndicator";

/**
 * RunDetailSidebar
 *
 * Left rail used on `/jobs/[id]/runs/[runId]`. Mirrors the visual idiom of
 * `JobDetailSidebar`: sticky, full-height, scrollable body, design tokens
 * only. Header section shows a back link to the parent workflow, the
 * workflow name, and the cron schedule. Body lists the steps that actually
 * ran (from `run.steps`) — clicking a row selects that step and the parent
 * page scrolls the log viewer to the step's starting offset.
 *
 * The "{ranCount} of {totalCount} ran" suffix only appears when totalCount
 * is known and differs from ranCount.
 */

export interface RunDetailSidebarProps {
  jobId: string;
  workflowName: string;
  cron: string;
  runSteps: WorkflowRunStep[];
  /** Total step count from the workflow definition. `null` while loading or if the workflow is deleted. */
  totalSteps: number | null;
  /** Currently selected step index (into `runSteps`), or null when none. */
  activeStepIndex: number | null;
  onSelectStep: (stepIndex: number) => void;
}

function formatStepCost(cost: number | null | undefined): string {
  if (cost == null || cost <= 0) return "—";
  return `$${cost.toFixed(3)}`;
}

function stepDurationLabel(step: WorkflowRunStep): string {
  if (!step.finished_at) return "—";
  const start = new Date(step.started_at).getTime();
  const end = new Date(step.finished_at).getTime();
  if (!Number.isFinite(start) || !Number.isFinite(end) || end <= start) {
    return "—";
  }
  return formatDuration(end - start);
}

export function RunDetailSidebar({
  jobId,
  workflowName,
  cron,
  runSteps,
  totalSteps,
  activeStepIndex,
  onSelectStep,
}: RunDetailSidebarProps) {
  const ranCount = runSteps.length;
  const showSuffix = totalSteps !== null && totalSteps !== ranCount;

  return (
    <aside className="h-full flex flex-col">
      {/* Header block */}
      <div className="shrink-0 p-3 flex flex-col gap-1">
        <Button
          href={`/jobs/${jobId}`}
          intent="ghost"
          size="sm"
          fullWidth
          icon={<ChevronLeft size={14} />}
          className="!justify-start"
        >
          Back to {workflowName}
        </Button>

        <div className="flex flex-col gap-1 px-2 py-3 border-b border-border-subtle">
          <div className="text-base font-semibold text-fg truncate" title={workflowName}>
            {workflowName}
          </div>
          <div className="font-mono text-xs text-fg-muted truncate" title={cron}>
            {cron}
          </div>
        </div>

        {/* Section header row */}
        <div className="flex items-center justify-between px-2 pt-3">
          <span className="text-[11px] font-semibold text-fg-subtle uppercase tracking-wider">
            Steps
          </span>
          {showSuffix && (
            <span className="text-[11px] text-fg-muted">
              {ranCount} of {totalSteps} ran
            </span>
          )}
        </div>
      </div>

      {/* Scrollable step list */}
      <div className="overflow-y-auto flex-1 px-3 pb-3 flex flex-col gap-1">
        {runSteps.length === 0 ? (
          <div className="text-center py-6 text-fg-subtle text-xs">
            No steps have started yet
          </div>
        ) : (
          runSteps.map((step, index) => {
            const isActive = index === activeStepIndex;
            const state = apiStatusToJobState(step.status);
            const activeClasses = isActive
              ? "border-l-2 border-brand bg-surface-secondary"
              : "border-l-2 border-transparent hover:bg-surface-hover";
            return (
              <button
                key={`${step.step_index}-${step.step_id}`}
                type="button"
                onClick={() => onSelectStep(index)}
                className={`w-full flex items-center gap-2 px-2 py-2 rounded-input text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring ${activeClasses}`}
              >
                <JobStateIndicator state={state} variant="dot" size="sm" />
                <span
                  className="font-mono text-xs text-fg truncate flex-1"
                  title={step.step_id}
                >
                  {step.step_id}
                </span>
                <span className="text-[11px] text-fg-muted whitespace-nowrap">
                  {stepDurationLabel(step)}
                </span>
                <span className="font-mono text-[11px] text-fg-muted whitespace-nowrap">
                  {formatStepCost(step.cost_usd)}
                </span>
              </button>
            );
          })
        )}
      </div>
    </aside>
  );
}
