"use client";

/**
 * EditorSidebar
 *
 * Left rail used by both `/create` and `/workflows/[id]/edit`. Implements
 * the same unified sidebar anatomy as `JobsSidebar` and `JobDetailSidebar`
 * so the editor stops feeling like an island — same width
 * (`var(--width-sidebar)`), same `p-3 / gap-4` rhythm, same
 * `SidebarSectionHeader` eyebrow style, same right-edge `border-r
 * border-border-subtle` (owned by the page grid cell, matching how
 * `/workflows/[id]` is wired).
 *
 * Sections, top to bottom:
 *
 *   1. Back link             — SidebarBackLink. "Back to Workflows" in
 *                              create mode, "Back to Workflow" in edit
 *                              mode (pointing at the parent detail page).
 *   2. Identity block        — workflow **name** edited inline. Renders
 *                              as a heading-styled span by default;
 *                              clicking (or focusing via keyboard) swaps
 *                              in an input. Same pattern as the old
 *                              `EditableCrumb` in `CanvasBreadcrumb`,
 *                              just relocated and dressed to match the
 *                              sidebar identity slot. Replaces the
 *                              breadcrumb + the old `WorkflowNameInline`
 *                              that previously lived on the canvas.
 *   3. SCHEDULE section      — eyebrow header + PropertyRows for cron,
 *                              timezone, and enabled. The cron input
 *                              uses a mono face and shows a subtle
 *                              cron-to-English preview below the row.
 *                              The timezone value slot hosts a compact
 *                              searchable popover (ported from the old
 *                              `ScheduleCard`). The enabled value slot
 *                              hosts the shared `Toggle` primitive.
 *   4. Save section          — a `CompactActionButton` (primary intent)
 *                              labelled "Create Workflow" in create
 *                              mode or "Save Changes" in edit mode.
 *                              Disabled when the name is blank, the
 *                              workflow has no steps, or a submission
 *                              is in flight. Errors render below the
 *                              button in the standard `text-status-failed`
 *                              line.
 *
 * The sidebar is purely presentational with respect to state: the parent
 * page owns the `NewWorkflow` (or its edit-mode equivalent), the editable
 * name, and the submit handler. This keeps it composable with both pages
 * without re-implementing the create/update wiring.
 */

import { useEffect, useRef, useState } from "react";
import { ChevronDown, Clock, Globe2 } from "lucide-react";
import { SidebarBackLink } from "./SidebarBackLink";
import { SidebarSectionHeader } from "./SidebarSectionHeader";
import { PropertyRow } from "@/components/ui/PropertyRow";
import { Toggle } from "@/components/ui/Toggle";
import { CompactActionButton } from "@/components/ui/CompactActionButton";
import { cronToEnglish } from "@/app/create/cron-english";
import { COMMON_TIMEZONES } from "@/app/create/timezones";

export type EditorSidebarMode =
  | { kind: "create" }
  | { kind: "edit"; workflowId: string };

export interface EditorSidebarProps {
  mode: EditorSidebarMode;
  /** Workflow name — controlled by the parent page. */
  name: string;
  onNameChange: (next: string) => void;
  /** Cron expression. */
  schedule: string;
  onScheduleChange: (next: string) => void;
  /** IANA timezone (or empty for "unset"). */
  timezone: string;
  onTimezoneChange: (next: string) => void;
  /** Enabled toggle. */
  enabled: boolean;
  onEnabledChange: (next: boolean) => void;
  /** Number of steps in the workflow; used for save-disabled state. */
  stepCount: number;
  /** Submit handler — wired to create/update. */
  onSubmit: () => void;
  /** True while a submission is in flight. */
  busy?: boolean;
  /** Optional error string rendered under the save button. */
  errorText?: string | null;
}

export function EditorSidebar({
  mode,
  name,
  onNameChange,
  schedule,
  onScheduleChange,
  timezone,
  onTimezoneChange,
  enabled,
  onEnabledChange,
  stepCount,
  onSubmit,
  busy = false,
  errorText = null,
}: EditorSidebarProps) {
  const isEdit = mode.kind === "edit";
  const backHref = isEdit ? `/workflows/${mode.workflowId}` : "/workflows";
  const backLabel = isEdit ? "Back to Workflow" : "Back to Workflows";

  const submitLabel = isEdit
    ? busy
      ? "Saving…"
      : "Save Changes"
    : busy
      ? "Creating…"
      : "Create Workflow";

  const english = cronToEnglish(schedule);
  const submitDisabled = busy || stepCount === 0 || !name.trim();

  return (
    <aside className="h-full flex flex-col">
      <div className="flex-1 overflow-y-auto p-3 flex flex-col gap-4">
        {/* 1. Back link */}
        <SidebarBackLink href={backHref}>{backLabel}</SidebarBackLink>

        {/* 2. Identity — editable workflow name */}
        <EditableNameBlock
          value={name}
          onChange={onNameChange}
          placeholder={isEdit ? "Untitled workflow" : "New workflow"}
        />

        {/* 3. SCHEDULE section */}
        <section className="flex flex-col gap-1.5">
          <SidebarSectionHeader title="Schedule" />
          <div className="flex flex-col">
            <PropertyRow
              label="Cron"
              value={
                <input
                  type="text"
                  value={schedule}
                  onChange={(e) => onScheduleChange(e.target.value)}
                  placeholder="0 9 * * *"
                  aria-label="Cron schedule"
                  className="font-mono text-xs text-fg bg-transparent border border-transparent hover:border-border-subtle focus:border-border focus:bg-surface-hover rounded-input px-1.5 py-1 outline-none focus:ring-2 focus:ring-brand-ring/40 text-right w-[140px]"
                />
              }
            />
            {/* Subtle cron-to-English preview under the cron row. */}
            <div className="px-2 -mt-1 pb-1 text-[11px] text-fg-muted text-right">
              {english}
            </div>
            <PropertyRow
              label="Timezone"
              value={
                <TimezonePopover value={timezone} onChange={onTimezoneChange} />
              }
            />
            <PropertyRow
              label="Enabled"
              value={
                <Toggle
                  checked={enabled}
                  onChange={onEnabledChange}
                  ariaLabel={enabled ? "Disable cron" : "Enable cron"}
                />
              }
            />
          </div>
        </section>

        {/* 4. Save section */}
        <section className="flex flex-col gap-1.5">
          <SidebarSectionHeader title={isEdit ? "Save" : "Create"} />
          <CompactActionButton
            intent="primary"
            fullWidth
            isDisabled={submitDisabled}
            onPress={onSubmit}
            ariaLabel={submitLabel}
          >
            {submitLabel}
          </CompactActionButton>
          {errorText && (
            <p className="px-1 text-xs text-status-failed">{errorText}</p>
          )}
        </section>
      </div>
    </aside>
  );
}

/* ── EditableNameBlock ────────────────────────────────────────────── */

interface EditableNameBlockProps {
  value: string;
  onChange: (next: string) => void;
  placeholder: string;
}

/**
 * The identity row equivalent for the editor — workflow name rendered
 * as a heading-weight line that flips to an input on click/focus.
 * Anatomy mirrors `SidebarIdentityBlock` (same `px-2 py-3 border-b
 * border-border-subtle` outer shell, same `text-base font-semibold`
 * type) so the editor's identity block visually aligns with the one
 * on `/workflows/[id]`.
 */
function EditableNameBlock({
  value,
  onChange,
  placeholder,
}: EditableNameBlockProps) {
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  return (
    <div className="px-2 py-3 border-b border-border-subtle">
      {editing ? (
        <input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onBlur={() => setEditing(false)}
          onKeyDown={(e) => {
            if (e.key === "Enter" || e.key === "Escape") {
              e.preventDefault();
              setEditing(false);
            }
          }}
          placeholder={placeholder}
          aria-label="Edit workflow name"
          className="w-full text-base font-semibold text-fg bg-surface-hover border border-border rounded-input px-2 py-1 outline-none focus:ring-2 focus:ring-brand-ring/40 focus:border-fg"
        />
      ) : (
        <button
          type="button"
          onClick={() => setEditing(true)}
          aria-label="Edit workflow name"
          className="w-full text-left text-base font-semibold text-fg truncate hover:bg-surface-hover rounded-input px-2 py-1 -mx-2 -my-1 cursor-text focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
          title={value || placeholder}
        >
          {value || (
            <span className="text-fg-subtle font-normal">{placeholder}</span>
          )}
        </button>
      )}
    </div>
  );
}

/* ── TimezonePopover ──────────────────────────────────────────────── */

interface TimezonePopoverProps {
  value: string;
  onChange: (next: string) => void;
}

/**
 * Compact, inline timezone picker for the sidebar PropertyRow value slot.
 * Ported from the old `ScheduleCard` so we don't lose the searchable list
 * UX, but tightened up to fit on the right side of a property row.
 */
function TimezonePopover({ value, onChange }: TimezonePopoverProps) {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState("");
  const ref = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onDown(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false);
    }
    document.addEventListener("mousedown", onDown);
    return () => document.removeEventListener("mousedown", onDown);
  }, [open]);

  const matches = COMMON_TIMEZONES.filter((z) =>
    z.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div ref={ref} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="inline-flex items-center gap-1.5 px-1.5 py-1 rounded-input hover:bg-surface-hover cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40 max-w-[160px]"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-label="Pick timezone"
      >
        <Globe2 size={12} className="text-fg-muted shrink-0" aria-hidden />
        <span className="text-xs text-fg truncate">
          {value || <span className="text-fg-subtle">no tz</span>}
        </span>
        <ChevronDown size={11} className="text-fg-subtle shrink-0" aria-hidden />
      </button>
      {open && (
        <div className="absolute top-full mt-1.5 right-0 z-30 w-[220px] bg-surface border border-border rounded-card shadow-menu">
          <div className="flex items-center gap-1.5 px-2 py-1.5 border-b border-border">
            <Clock size={12} className="text-fg-muted shrink-0" aria-hidden />
            <input
              autoFocus
              type="text"
              value={search}
              onChange={(e) => setSearch(e.target.value)}
              placeholder="Search timezones…"
              className="w-full text-xs bg-transparent outline-none text-fg placeholder:text-fg-subtle"
            />
          </div>
          <ul role="listbox" className="max-h-[220px] overflow-y-auto py-1">
            {value && !COMMON_TIMEZONES.includes(value) && (
              <li
                role="option"
                aria-selected={true}
                className="px-2.5 py-1.5 text-xs font-mono text-fg bg-surface-hover cursor-default"
              >
                {value} <span className="text-fg-muted">(current)</span>
              </li>
            )}
            {matches.map((tz) => (
              <li key={tz}>
                <button
                  type="button"
                  role="option"
                  aria-selected={tz === value}
                  onClick={() => {
                    onChange(tz);
                    setOpen(false);
                    setSearch("");
                  }}
                  className={[
                    "w-full text-left px-2.5 py-1.5 text-xs font-mono cursor-pointer",
                    tz === value
                      ? "bg-brand-muted text-brand-hover"
                      : "text-fg hover:bg-surface-hover",
                  ].join(" ")}
                >
                  {tz}
                </button>
              </li>
            ))}
            {matches.length === 0 && (
              <li className="px-2.5 py-2 text-[11px] text-fg-muted italic">
                No matches in common list.
              </li>
            )}
          </ul>
        </div>
      )}
    </div>
  );
}
