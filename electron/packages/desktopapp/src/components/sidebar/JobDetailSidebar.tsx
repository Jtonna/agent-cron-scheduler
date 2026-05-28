"use client";

import { useState } from "react";
import {
  ChevronLeft,
  Loader2,
  Pencil,
  Play,
  Power,
  Settings,
  Square,
  Star,
  StarOff,
  Trash2,
} from "lucide-react";
import { SidebarSection } from "./SidebarSection";
import { DeleteJobDialog } from "./DeleteJobDialog";
import { Button } from "@/components/ui/Button";
import { RunWithOverridesModal } from "@/components/jobs/RunWithOverridesModal";
import { useToggleWorkflowEnabled } from "@/apis/useToggleWorkflowEnabled";
import { useTriggerWorkflow } from "@/apis/useTriggerWorkflow";
import type { Job } from "@/apis/types";

/**
 * JobDetailSidebar
 *
 * Left rail used on `/jobs/[id]`. Top scroll region: back link, identity,
 * Actions section (Favorite / Edit / Enable-Disable Cron / Run Workflow /
 * Run with Overrides), and a conditional Run management section. The Delete
 * job button is pinned to the bottom of the sidebar via flex layout —
 * always visible regardless of scroll position.
 *
 * The Enable/Disable Cron, Run Workflow, and Run with Overrides buttons
 * are wired internally via the `useToggleWorkflowEnabled` and
 * `useTriggerWorkflow` hooks; the modal for overrides is also owned here.
 * Favorite, Stop, and Delete remain parent-controlled via `on*` callbacks
 * so consumers can wire them as needed (e.g., stop requires per-run kill
 * orchestration).
 *
 * Note: this component does not use the shared `Sidebar` shell because it
 * needs an internal scroll region + pinned footer; the page wrapper should
 * NOT add `overflow-y-auto` (the sidebar manages that internally).
 */

interface JobDetailSidebarProps {
  job: Job;
  /** Number of runs currently in flight for this job. Defaults to 0. */
  runningCount?: number;
  /** Whether the user has favorited this job. Defaults to false. */
  favorited?: boolean;
  onFavoriteToggle?: () => void;
  onStop?: () => void;
  /** Called from inside DeleteJobDialog after the user confirms. */
  onDelete?: () => void;
}

const NOOP = () => {};

export function JobDetailSidebar({
  job,
  runningCount = 0,
  favorited = false,
  onFavoriteToggle,
  onStop,
  onDelete,
}: JobDetailSidebarProps) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [overridesOpen, setOverridesOpen] = useState(false);

  const { toggle, toggling, error: toggleError } = useToggleWorkflowEnabled();
  const { trigger, triggering, error: triggerError } = useTriggerWorkflow();

  function handleToggleEnabled() {
    void toggle(job.id, job.enabled).catch(() => {
      // Error surfaces via toggleError below.
    });
  }

  function handleRunWorkflow() {
    void trigger(job.id, {}).catch(() => {
      // Error surfaces via triggerError below.
    });
  }

  const toggleLabel = job.enabled ? "Disable Cron" : "Enable Cron";

  return (
    <>
      <aside className="h-full flex flex-col">
        {/* Scrollable content */}
        <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-1">
          {/* Back button */}
          <Button
            href="/jobs"
            intent="ghost"
            size="sm"
            fullWidth
            icon={<ChevronLeft size={14} />}
            className="!justify-start"
          >
            Back to Jobs
          </Button>

          {/* Identity */}
          <div className="flex flex-col gap-1 px-2 py-3 border-b border-border-subtle">
            <div className="text-base font-semibold text-fg truncate">{job.name}</div>
            <div className="font-mono text-xs text-fg-muted truncate">{job.schedule}</div>
          </div>

          {/* Actions */}
          <SidebarSection title="Actions">
            <div className="flex flex-col gap-1">
              <Button
                fullWidth
                size="sm"
                intent="ghost"
                icon={favorited ? <Star size={14} fill="currentColor" /> : <StarOff size={14} />}
                onPress={onFavoriteToggle ?? NOOP}
              >
                {favorited ? "Favorited" : "Favorite"}
              </Button>
              <Button
                fullWidth
                size="sm"
                intent="ghost"
                icon={<Pencil size={14} />}
                href={`/jobs/${job.id}/edit`}
              >
                Edit
              </Button>
              <Button
                fullWidth
                size="sm"
                intent="ghost"
                icon={toggling ? <Loader2 size={14} className="animate-spin" /> : <Power size={14} />}
                onPress={handleToggleEnabled}
                isDisabled={toggling}
              >
                {toggleLabel}
              </Button>
              <Button
                fullWidth
                size="sm"
                icon={triggering ? <Loader2 size={14} className="animate-spin" /> : <Play size={14} />}
                onPress={handleRunWorkflow}
                isDisabled={triggering}
              >
                {triggering ? "Running…" : "Run Workflow"}
              </Button>
              <Button
                fullWidth
                size="sm"
                intent="secondary"
                icon={<Settings size={14} />}
                onPress={() => setOverridesOpen(true)}
              >
                Run with Overrides
              </Button>
              {(toggleError || triggerError) && (
                <p className="px-2 text-xs text-status-failed">
                  {toggleError ?? triggerError}
                </p>
              )}
            </div>
          </SidebarSection>

          {/* Run management — only when there are running runs */}
          {runningCount > 0 && (
            <SidebarSection title="Run management">
              <div className="flex flex-col gap-1">
                <Button
                  fullWidth
                  size="sm"
                  intent="secondary"
                  icon={<Square size={14} fill="currentColor" />}
                  onPress={onStop ?? NOOP}
                  className="!bg-status-failed-bg !text-status-failed hover:!bg-status-failed-bg/80"
                >
                  {runningCount === 1 ? "Stop run" : `Stop ${runningCount} runs`}
                </Button>
              </div>
            </SidebarSection>
          )}
        </div>

        {/* Pinned footer — Delete job stays at the bottom regardless of scroll */}
        <div className="shrink-0 p-3 border-t border-border-subtle">
          <Button
            fullWidth
            size="sm"
            intent="ghost"
            icon={<Trash2 size={14} />}
            className="!text-status-failed hover:!bg-status-failed-bg"
            onPress={() => setDeleteOpen(true)}
          >
            Delete job
          </Button>
        </div>
      </aside>

      <DeleteJobDialog
        isOpen={deleteOpen}
        onOpenChange={setDeleteOpen}
        jobName={job.name}
        onConfirm={onDelete ?? NOOP}
      />

      <RunWithOverridesModal
        workflow={job}
        isOpen={overridesOpen}
        onOpenChange={setOverridesOpen}
      />
    </>
  );
}
