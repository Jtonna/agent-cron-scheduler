"use client";

/**
 * StepEditorModal
 *
 * Linear-style modal for editing one step. Sits higher in the viewport
 * (10vh anchor), wider than the command palette (max-w-3xl) so the
 * larger editor bodies (match cases, agent prompts, http URLs) breathe.
 * Surface-secondary body, hairline header/footer separators, focus
 * restore on close.
 *
 * Portal: the modal is rendered via `createPortal` directly into
 * `document.body`. Without this, the modal would be parented inside the
 * reactflow canvas container — whose transformed/overflow-hidden
 * stacking context defeats `position: fixed` and traps the backdrop to
 * the canvas instead of covering the viewport.
 *
 * Owns:
 *   - header with kind-icon chip, breadcrumb-aware title, kind-switcher pill
 *   - scrollable body that dispatches to the right per-kind body editor
 *   - common Advanced collapsible section above the footer
 *   - footer with Cancel (left), keyboard hint (centre), Save Step (right)
 *
 * Edits flow via two callbacks:
 *   - `onChange(step)` — fires on every keystroke; the parent commits
 *     into its `NewWorkflow` state immediately so the canvas stays in
 *     sync (we don't have a separate "draft step" concept).
 *   - `onClose()` — fires on Cancel / Save / Escape / backdrop click.
 *
 * Nested editing (match cases) is handled at the parent level — this
 * modal calls `onOpenNested(caseKey | null)` when the user drills into
 * a case, and the parent stacks another StepEditorModal with its own
 * breadcrumb crumb.
 */

import { useEffect, useId, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { ChevronDown, X } from "lucide-react";
import type {
  NewAgentStep,
  NewHttpStep,
  NewMatchStep,
  NewScriptStep,
  NewSetVarStep,
  NewShellStep,
  NewStep,
} from "./types";
import { STEP_KIND_META } from "./stepMeta";
import { KindSwitcherPopover } from "./KindSwitcherPopover";
import { ShellStepBody } from "./editors/ShellStepBody";
import { ScriptStepBody } from "./editors/ScriptStepBody";
import { HttpStepBody } from "./editors/HttpStepBody";
import { MatchStepBody } from "./editors/MatchStepBody";
import { SetVarStepBody } from "./editors/SetVarStepBody";
import { AgentStepBody } from "./editors/AgentStepBody";
import { AdvancedSection } from "./editors/AdvancedSection";
import { FieldLabel, MonoTextInput } from "./editors/primitives";

export interface BreadcrumbCrumb {
  label: string;
  onClick?: () => void;
}

export interface StepEditorModalProps {
  step: NewStep;
  /** Breadcrumb crumbs leading up to (but not including) the step. */
  breadcrumb?: BreadcrumbCrumb[];
  onChange: (next: NewStep) => void;
  onClose: () => void;
  /** Drill into a child case (`null` for the default branch). Only used for match steps. */
  onOpenNested?: (caseKey: string | null) => void;
}

export function StepEditorModal({
  step,
  breadcrumb = [],
  onChange,
  onClose,
  onOpenNested,
}: StepEditorModalProps) {
  const labelId = useId();
  const meta = STEP_KIND_META[step.kind];
  const Icon = meta.Icon;
  const [switcherOpen, setSwitcherOpen] = useState(false);
  const previouslyFocusedRef = useRef<HTMLElement | null>(null);

  // Focus restore on close (CommandPalette pattern).
  useEffect(() => {
    previouslyFocusedRef.current = (document.activeElement as HTMLElement | null) ?? null;
    return () => {
      previouslyFocusedRef.current?.focus?.();
    };
  }, []);

  // Escape closes (the popover handles its own Escape; this is the global one).
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape" && !switcherOpen) {
        e.preventDefault();
        onClose();
      }
      // Cmd/Ctrl + Enter = save & close (no save mutation is wired here;
      // changes are already committed live, so we just close).
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
        e.preventDefault();
        onClose();
      }
    }
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose, switcherOpen]);

  // SSR guard — createPortal requires a real DOM node. Both consumer
  // pages declare `"use client"` so this only ever returns null during
  // the very brief server pass; the hydration render fills it in.
  if (typeof document === "undefined") return null;

  const overlay = (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-ink-900/60 pt-[10vh] px-4"
      style={{ backgroundColor: "rgba(17,24,39,0.6)" }}
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <span id={labelId} className="sr-only">
        Edit step
      </span>
      <div
        role="dialog"
        aria-modal="true"
        aria-labelledby={labelId}
        className="w-full max-w-3xl flex flex-col overflow-hidden rounded-menu border border-border bg-surface-secondary shadow-menu"
      >
        {/* ── Header ────────────────────────────────────────────────── */}
        <div className="relative flex items-center gap-2.5 h-12 px-3.5 border-b border-border">
          <span
            data-mesh={meta.mesh}
            className="inline-flex h-[22px] w-[22px] rounded-input items-center justify-center flex-shrink-0"
            aria-hidden
          >
            <Icon size={13} className="text-fg" strokeWidth={2.25} />
          </span>
          <div className="flex-1 min-w-0 text-[13.5px] text-fg truncate">
            <span className="text-fg-muted">Edit step:</span>{" "}
            {breadcrumb.length > 0 && (
              <>
                {breadcrumb.map((crumb, i) => (
                  <span key={i}>
                    {crumb.onClick ? (
                      <button
                        type="button"
                        onClick={crumb.onClick}
                        className="font-mono text-[12.5px] text-fg-muted hover:text-fg cursor-pointer"
                      >
                        {crumb.label}
                      </button>
                    ) : (
                      <span className="font-mono text-[12.5px] text-fg-muted">{crumb.label}</span>
                    )}
                    <span className="text-fg-subtle mx-1">/</span>
                  </span>
                ))}
              </>
            )}
            <span className="font-mono text-[13px] text-fg font-medium">{step.id || "untitled"}</span>
          </div>

          <button
            type="button"
            onClick={() => setSwitcherOpen((v) => !v)}
            className={[
              "inline-flex items-center gap-1 px-2.5 py-1 rounded-pill border text-[11px] font-mono cursor-pointer transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40",
              switcherOpen
                ? "bg-brand-muted border-brand text-brand-hover"
                : "bg-surface border-border text-fg hover:bg-surface-hover",
            ].join(" ")}
            aria-haspopup="menu"
            aria-expanded={switcherOpen}
          >
            {meta.label.toLowerCase()}
            <ChevronDown size={11} />
          </button>

          <button
            type="button"
            onClick={onClose}
            className="text-fg-subtle hover:text-fg hover:bg-surface-hover p-1.5 rounded-input cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
            aria-label="Close editor"
          >
            <X size={14} />
          </button>

          {switcherOpen && (
            <KindSwitcherPopover
              current={step}
              onPick={onChange}
              onClose={() => setSwitcherOpen(false)}
            />
          )}
        </div>

        {/* ── Body ──────────────────────────────────────────────────── */}
        <div className="max-h-[75vh] overflow-y-auto px-5 py-4">
          <div className="mb-4">
            <FieldLabel>Step id</FieldLabel>
            <MonoTextInput
              value={step.id}
              onChange={(e) => onChange({ ...step, id: e.target.value } as NewStep)}
              placeholder="step_id"
              autoFocus={step.id === ""}
            />
          </div>

          {step.kind === "shell" && (
            <ShellStepBody value={step as NewShellStep} onChange={onChange} />
          )}
          {step.kind === "script" && (
            <ScriptStepBody value={step as NewScriptStep} onChange={onChange} />
          )}
          {step.kind === "http" && (
            <HttpStepBody value={step as NewHttpStep} onChange={onChange} />
          )}
          {step.kind === "match" && (
            <MatchStepBody
              value={step as NewMatchStep}
              onChange={onChange}
              onDrillIn={(key) => onOpenNested?.(key)}
            />
          )}
          {step.kind === "set_var" && (
            <SetVarStepBody value={step as NewSetVarStep} onChange={onChange} />
          )}
          {step.kind === "agent" && (
            <AgentStepBody value={step as NewAgentStep} onChange={onChange} />
          )}

          <AdvancedSection step={step} onChange={onChange} />
        </div>

        {/* ── Footer ────────────────────────────────────────────────── */}
        <div className="flex items-center justify-between gap-3 px-4 py-3 border-t border-border bg-surface">
          <button
            type="button"
            onClick={onClose}
            className="text-fg-muted hover:text-fg hover:bg-surface-hover px-3.5 py-1.5 text-[12.5px] font-medium rounded-input cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
          >
            Cancel
          </button>
          <span className="text-[10.5px] font-mono text-fg-subtle">
            esc to dismiss · ⌘↵ to save
          </span>
          <button
            type="button"
            onClick={onClose}
            className="bg-brand hover:bg-brand-hover text-surface px-4 py-1.5 text-[12.5px] font-medium rounded-input cursor-pointer focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/60"
          >
            Save Step
          </button>
        </div>
      </div>
    </div>
  );

  return createPortal(overlay, document.body);
}
