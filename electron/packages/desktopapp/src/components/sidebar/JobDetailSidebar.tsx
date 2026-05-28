"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import {
  Button as AriaButton,
  Menu,
  MenuItem,
  MenuTrigger,
  Popover,
} from "react-aria-components";
import {
  ChevronDown,
  ChevronLeft,
  Loader2,
  MoreHorizontal,
  Pencil,
  Play,
  Power,
  Sliders,
  Star,
  Trash2,
} from "lucide-react";
import { DeleteJobDialog } from "./DeleteJobDialog";
import { SidebarSearchTrigger } from "./SidebarSearchTrigger";
import { RunWithCustomizationsModal } from "@/components/jobs/RunWithCustomizationsModal";
import { PropertyRow } from "@/components/ui/PropertyRow";
import { Toggle } from "@/components/ui/Toggle";
import {
  JobStateIndicator,
  apiStatusToJobState,
} from "@/components/ui/JobStateIndicator";
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
 * Left rail used on `/workflows/[id]`. Sectioned layout (Pattern C hybrid)
 * approved in the ACS-20 design discussion:
 *
 *   1. Back link
 *   2. Search trigger — a button styled as a search input. Clicking it
 *      opens the global command palette (Cmd/Ctrl+K from anywhere).
 *   3. Identity block — workflow name + cron + timezone, with an inline
 *      Favorite star + an overflow `…` menu (Edit · Delete).
 *   4. Compact split-button — Run Workflow + chevron → Run with
 *      Customizations.
 *   5. STATUS section — inline Enable Cron toggle plus read-only
 *      Schedule / Timezone / Next run / Last run rows.
 *   6. RECENT RUNS — the 6 latest runs, each a link to the run detail.
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
    // `toggle` flips the current value internally; we ignore `next`
    // because the mutation already knows what to do given the current
    // state. We still expose it via the Switch so React Aria gives us
    // the proper toggle UX.
    void next;
    void toggle(job.id, job.enabled).catch(() => {
      // Error surfaces via toggleError below.
    });
  }

  function handleRunWorkflow() {
    void trigger(job.id, {}).catch(() => {
      // Error surfaces via triggerError below.
    });
  }

  function handleFavorite() {
    if (favPending) return;
    void (favorited ? unfavorite(job.id) : favorite(job.id)).catch(() => {
      // Favorite errors are non-critical; SSE will reconcile state on retry.
    });
  }

  // ── Palette registration ──────────────────────────────────────────
  // Re-register whenever a label-affecting state bit flips. The
  // provider stores command lists by registration id; replacing the
  // list with a new register call yields the new labels.
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
        {/* Scrollable content */}
        <div className="flex-1 overflow-y-auto flex flex-col">
          {/* Back */}
          <div className="px-3 pt-3">
            <Link
              href="/workflows"
              className="inline-flex items-center gap-1.5 px-2 py-1 rounded-input text-xs text-fg-muted hover:text-fg hover:bg-surface-hover transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
            >
              <ChevronLeft size={14} />
              Back to Workflows
            </Link>
          </div>

          {/* Search trigger — opens the palette. */}
          <div className="px-3 mt-3">
            <SidebarSearchTrigger placeholder="Search · ⌘K" />
          </div>

          {/* Identity block — name, cron, timezone + favorite + overflow. */}
          <div className="mx-3 mt-3 px-2 py-3 border-b border-border-subtle">
            <div className="flex items-start gap-1">
              <div className="flex-1 min-w-0">
                <div
                  className="text-base font-semibold text-fg truncate"
                  title={job.name}
                >
                  {job.name}
                </div>
                <div
                  className="font-mono text-xs text-fg-muted truncate mt-0.5"
                  title={`${job.schedule} · ${job.timezone}`}
                >
                  {job.schedule} · {job.timezone}
                </div>
              </div>

              {/* Favorite — inline star. */}
              <button
                type="button"
                onClick={handleFavorite}
                disabled={favPending}
                aria-label={
                  favorited ? "Unfavorite workflow" : "Favorite workflow"
                }
                aria-pressed={favorited}
                className={[
                  "shrink-0 p-1.5 rounded-input outline-none focus-visible:ring-2 focus-visible:ring-brand-ring transition-colors cursor-pointer",
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

              {/* Overflow — Edit + Delete. */}
              <MenuTrigger>
                <AriaButton
                  type="button"
                  aria-label="More workflow actions"
                  className="shrink-0 p-1.5 -mr-1 rounded-input outline-none focus-visible:ring-2 focus-visible:ring-brand-ring hover:bg-surface-hover transition-colors cursor-pointer text-fg-subtle"
                >
                  <MoreHorizontal size={16} />
                </AriaButton>
                <Popover
                  placement="bottom end"
                  className="w-48 bg-surface border border-border rounded-menu shadow-menu py-1 z-50 outline-none entering:animate-in entering:fade-in entering:zoom-in-95 exiting:animate-out exiting:fade-out exiting:zoom-out-95"
                >
                  <Menu
                    className="outline-none"
                    onAction={(key) => {
                      if (key === "edit") {
                        router.push(`/workflows/${job.id}/edit`);
                      } else if (key === "delete") {
                        setDeleteOpen(true);
                      }
                    }}
                  >
                    <MenuItem
                      id="edit"
                      className="w-full flex items-center gap-2.5 px-3 py-2 text-xs text-fg-secondary hover:bg-surface-secondary outline-none cursor-pointer rounded-input mx-1"
                    >
                      <Pencil size={14} className="text-fg-tertiary" />
                      <span className="flex-1 text-left">Edit Workflow</span>
                    </MenuItem>
                    <div className="my-1 mx-2 border-t border-border-subtle" />
                    <MenuItem
                      id="delete"
                      className="w-full flex items-center gap-2.5 px-3 py-2 text-xs text-status-failed hover:bg-status-failed-bg outline-none cursor-pointer rounded-input mx-1"
                    >
                      <Trash2 size={14} />
                      <span className="flex-1 text-left">Delete Workflow…</span>
                    </MenuItem>
                  </Menu>
                </Popover>
              </MenuTrigger>
            </div>
          </div>

          {/* Compact split-button — Run Workflow + chevron. */}
          <div className="px-3 mt-3">
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
                {triggering ? "Running…" : "Run Workflow"}
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
              <p className="mt-2 px-1 text-xs text-status-failed">
                {toggleError ?? triggerError}
              </p>
            )}
          </div>

          {/* STATUS section */}
          <section className="px-3 mt-5">
            <div className="px-2 mb-1 text-[11px] font-semibold text-fg-subtle uppercase tracking-wider">
              Status
            </div>
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

          {/* RECENT RUNS section */}
          <section className="px-3 mt-5 mb-3">
            <div className="px-2 mb-1 flex items-center justify-between">
              <span className="text-[11px] font-semibold text-fg-subtle uppercase tracking-wider">
                Recent Runs
              </span>
              {runs.length > 0 && (
                <span className="text-[11px] text-fg-muted">
                  ({runs.length})
                </span>
              )}
            </div>
            {runs.length === 0 ? (
              <div className="px-2 py-3 text-xs text-fg-subtle">No runs yet</div>
            ) : (
              <div className="flex flex-col">
                {runs.slice(0, RECENT_RUNS_LIMIT).map((run) => (
                  <RecentRunRow key={run.run_id} jobId={job.id} run={run} />
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

/**
 * A single row in the Recent Runs section. Links to the run detail page,
 * shows the run's state dot, a short run id, and a relative timestamp.
 */
interface RecentRunRowProps {
  jobId: string;
  run: JobRun;
}

function RecentRunRow({ jobId, run }: RecentRunRowProps) {
  const state = apiStatusToJobState(run.status);
  const shortId = run.run_id.slice(0, 8);
  const when = formatTimeAgo(run.started_at);
  return (
    <Link
      href={`/workflows/${jobId}/runs/${run.run_id}`}
      className="w-full flex items-center gap-2 px-2 py-1.5 rounded-input text-xs hover:bg-surface-hover transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
    >
      <JobStateIndicator state={state} variant="dot" size="sm" />
      <span className="font-mono text-fg truncate flex-1" title={run.run_id}>
        #{shortId}
      </span>
      <span className="text-fg-muted whitespace-nowrap">{when}</span>
    </Link>
  );
}
