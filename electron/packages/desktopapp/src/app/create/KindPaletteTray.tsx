"use client";

/**
 * KindPaletteTray
 *
 * Left-edge dock of all six step kinds. Clicking a chip appends a step
 * of that kind at the end of the workflow. The tray is vertically
 * centred on the left edge so it coexists with the labeled minimap
 * stack pinned to the top-left without visual overlap.
 *
 * Chips advertise drag intent via `cursor-grab` (and `cursor-grabbing`
 * when active) so they look draggable even though the actual drag-onto-
 * canvas wiring is still pending — see the TODO below.
 *
 * TODO: true drag-from-tray-to-canvas (drop onto edges to insert
 * between, drop onto blank space to append). reactflow's external-drag
 * integration is non-trivial and was descoped from the prototype.
 */

import { STEP_KIND_META, STEP_KINDS } from "./stepMeta";
import type { StepKind } from "./types";

interface KindPaletteTrayProps {
  onAdd: (kind: StepKind) => void;
}

export function KindPaletteTray({ onAdd }: KindPaletteTrayProps) {
  return (
    <div
      role="toolbar"
      aria-label="Add step kind"
      className="absolute top-1/2 -translate-y-1/2 left-4 z-20 flex flex-col gap-1 bg-surface border border-border rounded-card shadow-menu p-1.5"
    >
      <div className="text-[9px] font-mono uppercase tracking-wider text-fg-subtle px-1 pb-1">
        Add
      </div>
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
            className="group inline-flex items-center justify-center h-8 w-8 rounded-input hover:bg-surface-hover active:bg-surface-tertiary cursor-grab active:cursor-grabbing transition-colors focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
          >
            <span
              data-mesh={meta.mesh}
              className="inline-flex h-6 w-6 rounded-input items-center justify-center"
            >
              <Icon size={13} className="text-fg" strokeWidth={2.25} />
            </span>
          </button>
        );
      })}
    </div>
  );
}
