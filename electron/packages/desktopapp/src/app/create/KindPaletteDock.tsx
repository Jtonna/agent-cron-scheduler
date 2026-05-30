"use client";

/**
 * KindPaletteDock
 *
 * Bottom-centre horizontal dock of all six step kinds. Replaces the
 * previous left-edge vertical `KindPaletteTray` to free the left side
 * for the new `EditorSidebar` and to align the editor with the rest of
 * the desktop app's chrome (sidebar on the left, canvas filling the
 * rest, floating action surfaces along the bottom).
 *
 * Visual contract: macOS-dock feel — rounded-card surface, soft shadow,
 * hairline border, ~24px bottom margin, each chip lifts on hover.
 * Chips are icon-only with a `title` tooltip; the kind label appears in
 * the tooltip so the dock stays compact. `cursor-grab` is preserved as
 * a drag-intent hint for the eventual drag-from-dock-to-canvas wiring
 * (see TODO below); clicking still appends the step at the end of the
 * workflow.
 *
 * TODO: true drag-from-dock-to-canvas (drop onto edges to insert
 * between, drop onto blank space to append). reactflow's external-drag
 * integration is non-trivial and was descoped from the prototype.
 */

import { STEP_KIND_META, STEP_KINDS } from "./stepMeta";
import type { StepKind } from "./types";

interface KindPaletteDockProps {
  onAdd: (kind: StepKind) => void;
}

export function KindPaletteDock({ onAdd }: KindPaletteDockProps) {
  return (
    <div
      role="toolbar"
      aria-label="Add step kind"
      className="absolute bottom-6 left-1/2 -translate-x-1/2 z-20 inline-flex items-center gap-1 bg-surface border border-border rounded-card shadow-menu px-2 py-1.5"
    >
      {STEP_KINDS.map((kind) => {
        const meta = STEP_KIND_META[kind];
        const Icon = meta.Icon;
        return (
          <button
            key={kind}
            type="button"
            onClick={() => onAdd(kind)}
            title={`Add ${meta.label} step`}
            aria-label={`Add ${meta.label} step`}
            className="group inline-flex items-center justify-center h-9 w-9 rounded-input hover:bg-surface-hover hover:-translate-y-0.5 active:translate-y-0 active:bg-surface-tertiary cursor-grab active:cursor-grabbing transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
          >
            <span
              data-mesh={meta.mesh}
              className="inline-flex h-7 w-7 rounded-input items-center justify-center"
            >
              <Icon size={14} className="text-fg" strokeWidth={2.25} />
            </span>
          </button>
        );
      })}
    </div>
  );
}
