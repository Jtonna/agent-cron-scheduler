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
 * the tooltip so the dock stays compact.
 *
 * Interaction (ACS-20 drag-from-dock):
 *
 *   - PRIMARY: HTML5 drag-and-drop. Each chip is `draggable`. On
 *     `onDragStart` the chip writes the kind into the dataTransfer
 *     payload under the `application/reactflow` MIME (the de-facto
 *     reactflow sidebar convention) and sets a custom drag image —
 *     a small node-shaped ghost rendered off-screen, mesh-tinted to the
 *     persona of the kind so the user sees "this is what's about to
 *     drop" follow the cursor. The canvas listens for `dragover` /
 *     `drop` and translates the drop coordinate to canvas space via
 *     `screenToFlowPosition`, placing the new step at exactly that
 *     position.
 *   - FALLBACK: plain `onClick` still appends the step at a scratch
 *     position (same affordance as before the drag UX). Users who
 *     prefer click-to-add don't lose that. The `onClick` only fires for
 *     a true click — HTML5 drag suppresses the click event by default.
 *
 * Ghost-image quirks worth knowing:
 *   - `setDragImage` requires the referenced element to be in the DOM
 *     at the moment `dragstart` fires AND to remain mounted long enough
 *     for the browser to snapshot it (a microtask is enough). We keep a
 *     hidden ghost per chip (absolute-positioned far off-screen) so it
 *     is always live and we never have to time DOM insertion against
 *     the drag handshake.
 *   - The offscreen ghost MUST be visible (non-`display:none`,
 *     non-`visibility:hidden`) for the browser to render it as the
 *     drag image. We hide it by translating it well outside the
 *     viewport instead. Safari is particularly strict on this.
 *   - The browser snapshots once per `dragstart`; visual changes to the
 *     ghost during the drag don't update the picture.
 */

import { useRef } from "react";
import { STEP_KIND_META, STEP_KINDS } from "./stepMeta";
import type { StepKind } from "./types";

/**
 * MIME used to carry the dragged step's kind through the HTML5 drag
 * data-transfer. Matches the reactflow community convention so anyone
 * familiar with their "drag-from-sidebar" recipe sees the same key.
 */
export const DOCK_DRAG_MIME = "application/reactflow";

interface KindPaletteDockProps {
  /**
   * Click-fallback handler — fires when the user clicks a chip without
   * starting a drag. Appends the step at a scratch position on the
   * canvas (legacy pre-ACS-20 behaviour, retained as a graceful
   * shortcut for users who prefer click-to-add).
   */
  onAdd: (kind: StepKind) => void;
}

export function KindPaletteDock({ onAdd }: KindPaletteDockProps) {
  // One off-screen ghost per chip so `setDragImage` always has a
  // mounted, visible element to snapshot. Keyed by kind so each chip
  // grabs its own ghost without sharing a single mutable ref.
  const ghostRefs = useRef<Map<StepKind, HTMLDivElement | null>>(new Map());

  function handleDragStart(e: React.DragEvent<HTMLButtonElement>, kind: StepKind) {
    e.dataTransfer.setData(DOCK_DRAG_MIME, kind);
    e.dataTransfer.effectAllowed = "move";
    const ghost = ghostRefs.current.get(kind);
    if (ghost) {
      // Offset puts the cursor roughly in the centre of the ghost card
      // so the drop point reads as "where the node will land".
      e.dataTransfer.setDragImage(ghost, 90, 22);
    }
  }

  return (
    <>
      {/* Off-screen ghost cards — one per kind. Translated far off the
          viewport so they don't render in the user's view, but the
          browser can still rasterise them for the drag image. We avoid
          `display:none` / `visibility:hidden` (both cause Safari and
          some Chromium builds to skip the snapshot entirely). */}
      <div
        aria-hidden
        className="fixed top-0 left-0 pointer-events-none"
        style={{ transform: "translate(-9999px, -9999px)" }}
      >
        {STEP_KINDS.map((kind) => {
          const meta = STEP_KIND_META[kind];
          const Icon = meta.Icon;
          return (
            <div
              key={kind}
              ref={(el) => {
                ghostRefs.current.set(kind, el);
              }}
              className="rounded-card bg-surface border border-border shadow-menu w-[180px] flex items-center gap-2 px-3 py-2"
            >
              <span
                data-mesh={meta.mesh}
                className="inline-flex h-6 w-6 items-center justify-center rounded-pill bg-surface/80 text-fg"
              >
                <Icon size={13} strokeWidth={2.25} />
              </span>
              <div className="text-[10px] font-mono tracking-wider uppercase text-fg-secondary">
                {meta.label}
              </div>
            </div>
          );
        })}
      </div>

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
              draggable
              onDragStart={(e) => handleDragStart(e, kind)}
              onClick={() => onAdd(kind)}
              title={`Drag ${meta.label} onto the canvas, or click to append`}
              aria-label={`Add ${meta.label} step`}
              className="group inline-flex items-center justify-center h-9 w-9 rounded-input hover:bg-surface-hover hover:-translate-y-0.5 active:translate-y-0 active:bg-surface-tertiary cursor-grab active:cursor-grabbing transition-all focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
            >
              <span
                data-mesh={meta.mesh}
                className="inline-flex h-7 w-7 rounded-input items-center justify-center pointer-events-none"
              >
                <Icon size={14} className="text-fg" strokeWidth={2.25} />
              </span>
            </button>
          );
        })}
      </div>
    </>
  );
}
