"use client";

import { formatDuration } from "@/apis/format";
import type { WorkflowRunStep } from "@/apis/types";
import { apiStatusToJobState } from "@/components/ui/JobStateIndicator";
import { SidebarBackLink } from "./SidebarBackLink";
import { SidebarIdentityBlock } from "./SidebarIdentityBlock";
import { SidebarSearchTrigger } from "./SidebarSearchTrigger";
import { SidebarSectionHeader } from "./SidebarSectionHeader";
import { SidebarListItem } from "./SidebarListItem";

/**
 * RunDetailSidebar
 *
 * Left rail used on `/workflows/[id]/runs/[runId]`. Implements the
 * unified sidebar anatomy shared with `JobsSidebar` and
 * `JobDetailSidebar`:
 *
 *   1. Back link             — SidebarBackLink ("Back to {workflowName}")
 *   2. Search trigger        — opens the global command palette (added
 *                              here for consistency with the other two
 *                              rails; the palette is global so the entry
 *                              point should be too)
 *   3. Identity block        — workflow name + cron line
 *   4. STEPS section         — eyebrow header + SidebarListItem rows
 *
 * Vertical rhythm matches the other rails: `gap-4` between sections,
 * `gap-1.5` between a section's header and its content.
 *
 * Step rows expose a leading status dot, the step id (mono, truncates),
 * a duration trailer, and a cost trailer — driven through the unified
 * `SidebarListItem` primitive.
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
      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
        {/* 1. Back link */}
        <SidebarBackLink href={`/workflows/${jobId}`}>
          Back to {workflowName}
        </SidebarBackLink>

        {/* 2. Search trigger */}
        <SidebarSearchTrigger placeholder="Search · ⌘K" />

        {/* 3. Identity block */}
        <SidebarIdentityBlock
          title={workflowName}
          meta={cron}
          monoMeta
        />

        {/* 4. STEPS section */}
        <section className="flex flex-col gap-1.5">
          <SidebarSectionHeader
            title="Steps"
            meta={showSuffix ? `${ranCount} of ${totalSteps} ran` : undefined}
          />
          {runSteps.length === 0 ? (
            <div className="px-2 py-2 text-xs text-fg-subtle italic">
              No steps have started yet
            </div>
          ) : (
            <div className="flex flex-col">
              {runSteps.map((step, index) => {
                const isActive = index === activeStepIndex;
                return (
                  <SidebarListItem
                    key={`${step.step_index}-${step.step_id}`}
                    state={apiStatusToJobState(step.status)}
                    title={step.step_id}
                    meta={stepDurationLabel(step)}
                    metaSecondary={formatStepCost(step.cost_usd)}
                    active={isActive}
                    onPress={() => onSelectStep(index)}
                  />
                );
              })}
            </div>
          )}
        </section>
      </div>
    </aside>
  );
}
