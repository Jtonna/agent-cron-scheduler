"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Button as AriaButton,
  Menu,
  MenuItem,
  MenuTrigger,
  Popover,
} from "react-aria-components";
import {
  ChevronDown,
  Loader2,
  Pencil,
  Play,
  Power,
  Sliders,
  Star,
  Trash2,
} from "lucide-react";
import { DeleteJobDialog } from "./DeleteJobDialog";
import { SidebarSearchTrigger } from "./SidebarSearchTrigger";
import { SidebarBackLink } from "./SidebarBackLink";
import { SidebarIdentityBlock } from "./SidebarIdentityBlock";
import { SidebarSectionHeader } from "./SidebarSectionHeader";
import { SidebarListItem } from "./SidebarListItem";
import { RunWithCustomizationsModal } from "@/components/jobs/RunWithCustomizationsModal";
import { CompactActionButton } from "@/components/ui/CompactActionButton";
import { PropertyRow } from "@/components/ui/PropertyRow";
import { Toggle } from "@/components/ui/Toggle";
import { apiStatusToJobState } from "@/components/ui/JobStateIndicator";
import { useCommandPalette } from "@/components/command-palette/useCommandPalette";
import { useToggleWorkflowEnabled } from "@/apis/useToggleWorkflowEnabled";
import { useTriggerWorkflow } from "@/apis/useTriggerWorkflow";
import { useFavorite } from "@/apis/useFavorite";
import { useJobRuns } from "@/apis/useJobRuns";
import { formatTimeAgo, formatTimeUntil } from "@/apis/format";
import type { Job, JobRun } from "@/apis/types";

/**
 * JobDetailSidebar
 *
 * Left rail used on `/workflows/[id]`. Implements the unified sidebar
 * anatomy shared with `JobsSidebar` and `RunDetailSidebar`:
 *
 *   1. Back link             — SidebarBackLink ("Back to Workflows")
 *   2. Search trigger        — opens the global command palette
 *   3. Identity block        — workflow name + cron + tz + favorite star
 *   4. Primary action row    — Run split button (CompactActionButton)
 *   5. Secondary action row  — Delete (icon-only, w-9, left) +
 *                              Edit (flex-1, right). Both render with
 *                              visible chrome so they read as siblings of
 *                              the Run split-button above. The sidebar
 *                              drops the "Workflow" suffix from these
 *                              labels because the page context already
 *                              tells you what's being acted on; the
 *                              command palette still says
 *                              "Run/Edit/Delete Workflow" since it's
 *                              global.
 *   6. STATUS section        — eyebrow header + PropertyRows (enabled toggle + meta)
 *   7. RECENT RUNS section   — eyebrow header + SidebarListItem rows, capped at 6
 *
 * Vertical rhythm is one `gap-4` token between sections; section
 * headers and their content sit inside the same flex column with a
 * tighter `gap-1.5`.
 *
 * The sidebar also registers a set of workflow-scoped commands into the
 * palette while mounted. The commands re-register whenever any label-
 * affecting bit changes (enabled, favorited) so the palette label stays
 * in sync with the current state. We unregister on unmount.
 */

interface JobDetailSidebarProps {
  job: Job;
  /** Called from inside DeleteJobDialog after the user confirms. */
  onDelete?: () => void;
  /**
   * Inject runs for stories/tests. When omitted, the sidebar fetches the
   * 6 most-recent runs itself via `useJobRuns`.
   */
  runsOverride?: JobRun[];
}

const NOOP = () => {};
const RECENT_RUNS_LIMIT = 6;

export function JobDetailSidebar({
  job,
  onDelete,
  runsOverride,
}: JobDetailSidebarProps) {
  const router = useRouter();
  const palette = useCommandPalette();

  const [deleteOpen, setDeleteOpen] = useState(false);
  const [customizeOpen, setCustomizeOpen] = useState(false);

  const { toggle, toggling, error: toggleError } = useToggleWorkflowEnabled();
  const { trigger, triggering, error: triggerError } = useTriggerWorkflow();
  const { favorite, unfavorite, isPending: favPending } = useFavorite();

  // Recent runs (fetched here so the sidebar is self-contained). Stories
  // can override by passing `runsOverride`.
  const { runs: fetchedRuns } = useJobRuns(
    runsOverride ? "" : job.id,
    RECENT_RUNS_LIMIT,
  );
  const runs = runsOverride ?? fetchedRuns;

  const favorited = job.is_favorited;

  function handleToggleEnabled(next: boolean) {
    void next;
    void toggle(job.id, job.enabled).catch(() => {});
  }

  function handleRunWorkflow() {
    void trigger(job.id, {}).catch(() => {});
  }

  function handleFavorite() {
    if (favPending) return;
    void (favorited ? unfavorite(job.id) : favorite(job.id)).catch(() => {});
  }

  // ── Palette registration ──────────────────────────────────────────
  useEffect(() => {
    const id = palette.registerCommands([
      {
        id: "workflow:run",
        group: "Workflow Actions",
        label: "Run Workflow",
        icon: <Play size={14} />,
        action: () => {
          void trigger(job.id, {}).catch(() => {});
        },
      },
      {
        id: "workflow:run-custom",
        group: "Workflow Actions",
        label: "Run with Customizations…",
        icon: <Sliders size={14} />,
        action: () => setCustomizeOpen(true),
      },
      {
        id: "workflow:toggle-cron",
        group: "Workflow Actions",
        label: job.enabled ? "Disable Cron" : "Enable Cron",
        icon: <Power size={14} />,
        action: () => {
          void toggle(job.id, job.enabled).catch(() => {});
        },
      },
      {
        id: "workflow:edit",
        group: "Workflow Actions",
        label: "Edit Workflow",
        icon: <Pencil size={14} />,
        action: () => router.push(`/workflows/${job.id}/edit`),
      },
      {
        id: "workflow:favorite",
        group: "Workflow Actions",
        label: favorited ? "Unfavorite" : "Favorite this workflow",
        icon: <Star size={14} />,
        action: () => {
          void (favorited ? unfavorite(job.id) : favorite(job.id)).catch(
            () => {},
          );
        },
      },
      {
        id: "workflow:delete",
        group: "Workflow Actions",
        label: "Delete Workflow…",
        icon: <Trash2 size={14} />,
        action: () => setDeleteOpen(true),
      },
    ]);
    return () => palette.unregisterCommands(id);
  }, [
    palette,
    router,
    trigger,
    toggle,
    favorite,
    unfavorite,
    job.id,
    job.enabled,
    favorited,
  ]);

  const nextRunLabel = job.next_run_at ? formatTimeUntil(job.next_run_at) : "—";
  const lastRunLabel = job.last_run_at ? formatTimeAgo(job.last_run_at) : "—";

  return (
    <>
      <aside className="h-full flex flex-col">
        <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
          {/* 1. Back link */}
          <SidebarBackLink href="/workflows">Back to Workflows</SidebarBackLink>

          {/* 2. Search trigger */}
          <SidebarSearchTrigger placeholder="Search · ⌘K" />

          {/* 3. Identity block */}
          <SidebarIdentityBlock
            title={job.name}
            meta={`${job.schedule} · ${job.timezone}`}
            monoMeta
            actions={
              <button
                type="button"
                onClick={handleFavorite}
                disabled={favPending}
                aria-label={
                  favorited ? "Unfavorite workflow" : "Favorite workflow"
                }
                aria-pressed={favorited}
                className={[
                  "p-1.5 rounded-input outline-none focus-visible:ring-2 focus-visible:ring-brand-ring transition-colors cursor-pointer",
                  favPending
                    ? "opacity-50 cursor-not-allowed"
                    : "hover:bg-surface-hover",
                ].join(" ")}
              >
                <Star
                  size={16}
                  className={
                    favorited ? "fill-brand text-brand" : "text-fg-subtle"
                  }
                />
              </button>
            }
          />

          {/* 4. Primary action row — Run Workflow split button */}
          <div className="flex flex-col gap-2">
            <div
              role="group"
              aria-label="Run workflow"
              className={[
                "flex w-full rounded-input overflow-hidden",
                triggering ? "opacity-90" : "",
              ].join(" ")}
            >
              <AriaButton
                type="button"
                onPress={handleRunWorkflow}
                isDisabled={triggering}
                aria-label="Run workflow with default arguments"
                className={[
                  "flex-1 inline-flex items-center justify-center gap-1.5",
                  "px-3 py-1.5 text-xs font-semibold",
                  "bg-brand hover:bg-brand-hover text-surface",
                  "outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2",
                  "cursor-pointer transition-colors",
                  "disabled:opacity-50 disabled:cursor-not-allowed",
                  "rounded-l-input rounded-r-none",
                ].join(" ")}
              >
                {triggering ? (
                  <Loader2 size={12} className="animate-spin" />
                ) : (
                  <Play size={12} />
                )}
                {triggering ? "Running…" : "Run"}
              </AriaButton>

              <div aria-hidden className="w-px bg-brand-hover/60" />

              <MenuTrigger>
                <AriaButton
                  type="button"
                  isDisabled={triggering}
                  aria-label="More run options"
                  className={[
                    "inline-flex items-center justify-center",
                    "w-8 px-0 py-1.5",
                    "bg-brand hover:bg-brand-hover text-surface",
                    "outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2",
                    "cursor-pointer transition-colors",
                    "disabled:opacity-50 disabled:cursor-not-allowed",
                    "rounded-r-input rounded-l-none",
                  ].join(" ")}
                >
                  <ChevronDown size={12} aria-hidden />
                </AriaButton>
                <Popover
                  placement="bottom end"
                  className="w-56 bg-surface border border-border rounded-menu shadow-menu py-1 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95"
                >
                  <Menu
                    className="outline-none"
                    onAction={(key) => {
                      if (key === "customize") setCustomizeOpen(true);
                    }}
                  >
                    <MenuItem
                      id="customize"
                      className="w-full flex items-center gap-2.5 px-3 py-2 text-xs text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer rounded-input mx-1"
                    >
                      <Sliders size={14} className="text-fg-tertiary" />
                      <span className="flex-1 text-left">
                        Run with customizations…
                      </span>
                    </MenuItem>
                  </Menu>
                </Popover>
              </MenuTrigger>
            </div>
            {(toggleError || triggerError) && (
              <p className="px-1 text-xs text-status-failed">
                {toggleError ?? triggerError}
              </p>
            )}
          </div>

          {/* 5. Secondary action row — Delete (icon-only, left) + Edit (right) */}
          <div className="flex gap-2">
            <CompactActionButton
              intent="destructive"
              className="w-9 px-0 shrink-0"
              icon={<Trash2 size={12} />}
              onPress={() => setDeleteOpen(true)}
              ariaLabel="Delete workflow"
            >
              {null}
            </CompactActionButton>
            <CompactActionButton
              intent="secondary"
              fullWidth
              icon={<Pencil size={12} />}
              onPress={() => router.push(`/workflows/${job.id}/edit`)}
              ariaLabel="Edit workflow"
            >
              Edit
            </CompactActionButton>
          </div>

          {/* 6. STATUS section */}
          <section className="flex flex-col gap-1.5">
            <SidebarSectionHeader title="Status" />
            <div className="flex flex-col">
              <PropertyRow
                label="Enabled"
                value={
                  <Toggle
                    checked={job.enabled}
                    onChange={handleToggleEnabled}
                    disabled={toggling}
                    ariaLabel={job.enabled ? "Disable cron" : "Enable cron"}
                  />
                }
              />
              <PropertyRow label="Schedule" value={job.schedule} mono />
              <PropertyRow label="Timezone" value={job.timezone} />
              <PropertyRow label="Next run" value={nextRunLabel} />
              <PropertyRow label="Last run" value={lastRunLabel} />
            </div>
          </section>

          {/* 7. RECENT RUNS section */}
          <section className="flex flex-col gap-1.5">
            <SidebarSectionHeader
              title="Recent Runs"
              meta={runs.length > 0 ? `(${runs.length})` : undefined}
            />
            {runs.length === 0 ? (
              <div className="px-2 py-2 text-xs text-fg-subtle italic">
                No runs yet
              </div>
            ) : (
              <div className="flex flex-col">
                {runs.slice(0, RECENT_RUNS_LIMIT).map((run) => (
                  <SidebarListItem
                    key={run.run_id}
                    state={apiStatusToJobState(run.status)}
                    title={`#${run.run_id.slice(0, 8)}`}
                    titleTooltip={run.run_id}
                    meta={formatTimeAgo(run.started_at)}
                    href={`/workflows/${job.id}/runs/${run.run_id}`}
                  />
                ))}
              </div>
            )}
          </section>
        </div>
      </aside>

      <DeleteJobDialog
        isOpen={deleteOpen}
        onOpenChange={setDeleteOpen}
        jobName={job.name}
        onConfirm={onDelete ?? NOOP}
      />

      <RunWithCustomizationsModal
        workflow={job}
        isOpen={customizeOpen}
        onOpenChange={setCustomizeOpen}
      />
    </>
  );
}
