"use client";

/**
 * TailPlusButton
 *
 * Per-chain tail `+` affordance — a small circular button floating to
 * the right of the tail node of every chain (top-level chain, each
 * match-case branch, the match-default branch). Companion to
 * `EdgePlusButton`: that one inserts BETWEEN two existing nodes; this
 * one APPENDS after the last node of a chain that has no successor.
 *
 * Why this exists: the mid-edge `+` only appears between existing
 * nodes, so before ACS-20 a user could not extend a match-case branch
 * once they'd reached its tail (no edge to host the picker). With
 * `TailPlusButton` every chain has an explicit "add to this branch"
 * affordance — see the agent step at the tail of `compose_happy` in
 * the weather-greeter demo for the canonical case.
 *
 * Mount strategy: rendered as an absolutely-positioned div inside the
 * reactflow viewport (the `.react-flow__viewport` element). This makes
 * the buttons pan and zoom with the canvas — they're conceptually
 * attached to a node, not the editor chrome. Coordinates are derived
 * from the tail node's persisted position + node width. We deliberately
 * sidestep reactflow's `<NodeToolbar>` because it only renders for
 * SELECTED nodes; this button must always be visible.
 *
 * On click: opens the shared `KindPickerPopover` portalled to body.
 * On pick: calls `onAppend(scope, kind)` — the editor handles the
 * actual `appendStepToScope` mutation and the auto-wire (the new node
 * is NOT marked disconnected, because the user's intent to wire it
 * after the tail is explicit).
 *
 * Suppression: tail nodes that are themselves `match` steps are
 * skipped — a `match` step is inherently a fan-out point, not a "next
 * step" host, so a tail-+ on top of a match would be semantically
 * confused (which branch would it append into?). The user must use
 * the in-branch tail-+ instead.
 */

import { useRef, useState } from "react";
import { Plus } from "lucide-react";
import { KindPickerPopover } from "./KindPickerPopover";
import type { ChainScope } from "./graphPaths";
import type { StepKind } from "./types";

/** Horizontal gap between the node's right edge and the button. */
const BUTTON_GAP = 16;
/** Button diameter (matches `EdgePlusButton`'s footprint). */
const BUTTON_SIZE = 24;
/** Assumed StepNode dimensions for vertical centring; mirrored from
 * `graph.ts`'s `NODE_WIDTH` / `NODE_HEIGHT` constants. */
const NODE_WIDTH = 240;
const NODE_HEIGHT = 92;

export interface TailPlusButtonProps {
  /** Tail node's top-left position in flow coordinates. */
  position: { x: number; y: number };
  /** Scope to append into when a kind is picked. */
  scope: ChainScope;
  /** Stable id used as a React key + ARIA descriptor. */
  tailNodeId: string;
  /** Called when the user picks a kind from the popover. */
  onAppend: (scope: ChainScope, kind: StepKind) => void;
}

export function TailPlusButton({ position, scope, tailNodeId, onAppend }: TailPlusButtonProps) {
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [open, setOpen] = useState(false);

  // Vertically centre on the node, anchored to its right edge + gap.
  const left = position.x + NODE_WIDTH + BUTTON_GAP;
  const top = position.y + NODE_HEIGHT / 2 - BUTTON_SIZE / 2;

  return (
    <>
      <button
        ref={buttonRef}
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          setOpen((v) => !v);
        }}
        // Mounted inside the reactflow viewport so it pans/zooms with
        // the canvas. `pointer-events: all` so the button is clickable
        // even though the viewport's children typically defer to
        // reactflow's own event delegation.
        style={{
          position: "absolute",
          left,
          top,
          width: BUTTON_SIZE,
          height: BUTTON_SIZE,
          pointerEvents: "all",
        }}
        className="inline-flex items-center justify-center rounded-full bg-surface border-2 border-dashed border-brand text-brand hover:bg-brand hover:text-surface hover:border-solid cursor-pointer shadow-sm transition-colors"
        aria-label={`Append step after ${tailNodeId}`}
        title="Append step"
      >
        <Plus size={12} strokeWidth={2.5} />
      </button>
      {open && (
        <KindPickerPopover
          anchorRef={buttonRef}
          onPick={(kind) => onAppend(scope, kind)}
          onClose={() => setOpen(false)}
        />
      )}
    </>
  );
}
