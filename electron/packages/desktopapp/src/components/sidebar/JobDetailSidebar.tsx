"use client";

import { useState } from "react";
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
  Pencil,
  Play,
  Power,
  Sliders,
  Star,
  Trash2,
} from "lucide-react";
import { DeleteJobDialog } from "./DeleteJobDialog";
import { Button } from "@/components/ui/Button";
import { RunWithCustomizationsModal } from "@/components/jobs/RunWithCustomizationsModal";
import { useToggleWorkflowEnabled } from "@/apis/useToggleWorkflowEnabled";
import { useTriggerWorkflow } from "@/apis/useTriggerWorkflow";
import { useFavorite } from "@/apis/useFavorite";
import type { Job } from "@/apis/types";

/**
 * JobDetailSidebar
 *
 * Left rail used on `/workflows/[id]`. Visual hierarchy follows the
 * RunDetailSidebar idiom:
 *
 *   1. Back link (ghost square, top-aligned, sticky)
 *   2. Identity block (job name + cron) — bordered, no buttons
 *   3. Metadata strip (favorite + enable toggle) — compact icon-style row
 *   4. Primary action — a split button: a large "Run Workflow" on the
 *      left, joined to a small chevron trigger on the right that opens a
 *      menu containing "Run with customizations…". Inspired by shadcn's
 *      button-group / split-button pattern (and IDE-style Run controls).
 *   5. Utility actions — Enable/Disable Cron, Edit (ghost square rows)
 *   6. Pinned footer — Delete job (destructive ghost square)
 *
 * Shape is uniform across the entire rail: every interactive control uses
 * `rounded-input` (square corners). Intent (color/background) is what
 * distinguishes primary vs. secondary vs. ghost vs. destructive — never
 * shape. This matches the visual rhythm set by the "Run with
 * Customizations" CTA and the unified spec in
 * `src/components/sidebar/RunDetailSidebar.tsx`.
 *
 * The favorite control is self-wired via `useFavorite()` (no `onFavorite`
 * prop required) so consumers can't accidentally leave the toggle dead.
 * Enable/Disable Cron, Run Workflow, and the customizations modal are
 * also self-wired via their respective hooks.
 *
 * The previous "Run management" section that surfaced live-run stop
 * controls has been removed — the runs table on the page is the canonical
 * place to manage in-flight runs (each row has its own kill button).
 *
 * Note: this component does not use the shared `Sidebar` shell because it
 * needs an internal scroll region + pinned footer; the page wrapper should
 * NOT add `overflow-y-auto` (the sidebar manages that internally).
 */

interface JobDetailSidebarProps {
  job: Job;
  /** Called from inside DeleteJobDialog after the user confirms. */
  onDelete?: () => void;
}

const NOOP = () => {};

export function JobDetailSidebar({ job, onDelete }: JobDetailSidebarProps) {
  const [deleteOpen, setDeleteOpen] = useState(false);
  const [customizeOpen, setCustomizeOpen] = useState(false);

  const { toggle, toggling, error: toggleError } = useToggleWorkflowEnabled();
  const { trigger, triggering, error: triggerError } = useTriggerWorkflow();
  const { favorite, unfavorite, isPending: favPending } = useFavorite();

  const favorited = job.is_favorited;

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

  function handleFavorite() {
    if (favPending) return;
    void (favorited ? unfavorite(job.id) : favorite(job.id)).catch(() => {
      // Favorite errors are non-critical; SSE will reconcile state on retry.
    });
  }

  const toggleLabel = job.enabled ? "Disable cron" : "Enable cron";

  return (
    <>
      <aside className="h-full flex flex-col">
        {/* Scrollable content */}
        <div className="flex-1 overflow-y-auto flex flex-col">
          {/* Back */}
          <div className="px-3 pt-3">
            <Button
              href="/workflows"
              intent="ghost"
              size="sm"
              shape="rounded"
              fullWidth
              icon={<ChevronLeft size={14} />}
              className="!justify-start"
            >
              Back to Workflows
            </Button>
          </div>

          {/* Identity — name, schedule. Mirrors RunDetailSidebar header. */}
          <div className="mx-3 mt-2 px-2 py-3 border-b border-border-subtle">
            <div className="flex items-start gap-2">
              <div className="flex-1 min-w-0">
                <div className="text-base font-semibold text-fg truncate" title={job.name}>
                  {job.name}
                </div>
                <div className="font-mono text-xs text-fg-muted truncate mt-0.5" title={job.schedule}>
                  {job.schedule}
                </div>
              </div>
              {/* Favorite — icon-only, top-right of identity block.
                  Self-wired via useFavorite so it works without a prop. */}
              <button
                type="button"
                onClick={handleFavorite}
                disabled={favPending}
                aria-label={favorited ? "Unfavorite workflow" : "Favorite workflow"}
                aria-pressed={favorited}
                className={[
                  "shrink-0 p-1.5 -mr-1 rounded-input outline-none focus-visible:ring-2 focus-visible:ring-brand-ring transition-colors cursor-pointer",
                  favPending ? "opacity-50 cursor-not-allowed" : "hover:bg-surface-hover",
                ].join(" ")}
              >
                <Star
                  size={16}
                  className={favorited ? "fill-brand text-brand" : "text-fg-subtle"}
                />
              </button>
            </div>
          </div>

          {/* Primary action — split button. The left half is the headline
              "Run Workflow" action; the right half is a chevron trigger
              that opens a tiny menu offering the customizations modal.
              Both halves share one outer border (rounded-input) and one
              continuous hover/focus rhythm — a single cohesive control.

              Keyboard: Tab moves between the two halves. On the chevron,
              Enter/Space/Arrow-Down opens the menu; Arrow keys + Enter
              select; Escape closes (handled by react-aria-components). */}
          <div className="px-3 mt-4">
            <div
              role="group"
              aria-label="Run workflow"
              className={[
                "flex w-full rounded-input overflow-hidden",
                "ring-1 ring-brand/0", // reserve a slot for focus-within emphasis
                triggering ? "opacity-90" : "",
              ].join(" ")}
            >
              <AriaButton
                type="button"
                onPress={handleRunWorkflow}
                isDisabled={triggering}
                aria-label="Run workflow with default arguments"
                className={[
                  "flex-1 inline-flex items-center justify-center gap-2",
                  "px-5 py-2.5 text-sm font-semibold",
                  "bg-brand hover:bg-brand-hover text-surface",
                  "outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2",
                  "cursor-pointer transition-colors",
                  "disabled:opacity-50 disabled:cursor-not-allowed",
                  // square the right edge so it meets the chevron seamlessly
                  "rounded-l-input rounded-r-none",
                ].join(" ")}
              >
                {triggering ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Play size={14} />
                )}
                {triggering ? "Running…" : "Run Workflow"}
              </AriaButton>

              {/* 1px seam between the two halves — uses brand-hover so it
                  blends with the primary fill without looking like a double
                  border. */}
              <div aria-hidden className="w-px bg-brand-hover/60" />

              <MenuTrigger>
                <AriaButton
                  type="button"
                  isDisabled={triggering}
                  aria-label="More run options"
                  className={[
                    "inline-flex items-center justify-center",
                    "w-9 px-0 py-2.5",
                    "bg-brand hover:bg-brand-hover text-surface",
                    "outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2",
                    "cursor-pointer transition-colors",
                    "disabled:opacity-50 disabled:cursor-not-allowed",
                    "rounded-r-input rounded-l-none",
                  ].join(" ")}
                >
                  <ChevronDown size={14} aria-hidden />
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

          {/* Utility actions — quieter ghost rows.
              These don't compete with the primary CTA above. */}
          <nav className="px-3 mt-5 flex flex-col">
            <SidebarRow
              label={toggleLabel}
              icon={
                toggling ? (
                  <Loader2 size={14} className="animate-spin" />
                ) : (
                  <Power size={14} />
                )
              }
              onPress={handleToggleEnabled}
              disabled={toggling}
            />
            <SidebarRow
              label="Edit workflow"
              icon={<Pencil size={14} />}
              href={`/workflows/${job.id}/edit`}
            />
          </nav>
        </div>

        {/* Pinned footer — destructive action lives alone, away from
            the primary buttons, to reduce mis-click risk. */}
        <div className="shrink-0 px-3 py-3 border-t border-border-subtle">
          <SidebarRow
            label="Delete workflow"
            icon={<Trash2 size={14} />}
            onPress={() => setDeleteOpen(true)}
            destructive
          />
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
 * Internal utility-row used for low-prominence actions (Edit, Enable/Disable
 * cron, Delete). Renders as a left-aligned, squared, ghost row that hovers
 * to a subtle surface — visually closer to a nav item than a button, which
 * is the point: secondary actions shouldn't compete with the primary CTA.
 */
interface SidebarRowProps {
  label: string;
  icon: React.ReactNode;
  href?: string;
  onPress?: () => void;
  disabled?: boolean;
  destructive?: boolean;
}

function SidebarRow({
  label,
  icon,
  href,
  onPress,
  disabled,
  destructive,
}: SidebarRowProps) {
  const base =
    "w-full flex items-center gap-2 px-2 py-2 rounded-input text-xs font-medium outline-none focus-visible:ring-2 focus-visible:ring-brand-ring transition-colors cursor-pointer";
  const colorClasses = destructive
    ? "text-status-failed hover:bg-status-failed-bg"
    : "text-fg-secondary hover:text-fg hover:bg-surface-hover";
  const disabledClasses = disabled ? "opacity-50 cursor-not-allowed" : "";
  const cls = [base, colorClasses, disabledClasses].filter(Boolean).join(" ");

  if (href) {
    return (
      <Link href={href} className={cls}>
        <span className="text-fg-tertiary">{icon}</span>
        <span>{label}</span>
      </Link>
    );
  }

  return (
    <button type="button" onClick={onPress} disabled={disabled} className={cls}>
      <span className={destructive ? "text-status-failed" : "text-fg-tertiary"}>
        {icon}
      </span>
      <span>{label}</span>
    </button>
  );
}
