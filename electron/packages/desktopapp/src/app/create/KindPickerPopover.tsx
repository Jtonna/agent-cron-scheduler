"use client";

/**
 * KindPickerPopover
 *
 * Portalled `KindPicker` anchored to a trigger element. Extracted from
 * `EdgePlusButton` so both the mid-edge `+` button and the per-chain
 * tail-`+` button can share one popover implementation (same portal,
 * same viewport-clamp behaviour, same dismissal semantics).
 *
 * Why portal: rendering the picker inside the reactflow canvas (or
 * inside the `EdgeLabelRenderer`) drops it behind neighbouring nodes
 * because reactflow's transformed canvas establishes a stacking
 * context that the popover can't escape with z-index alone. Portalling
 * to `document.body` sidesteps the whole problem. Same family of fix
 * as `StepEditorModal`'s portal (commit ebc820a).
 *
 * Dismissal: clicking the scrim closes the popover; clicking inside
 * the picker does not. `onClose` is called both from explicit picks
 * (after `onPick`) and from scrim clicks.
 */

import { type RefObject } from "react";
import { createPortal } from "react-dom";
import { KindPicker } from "./KindPicker";
import { usePopoverAnchor } from "./usePopoverAnchor";
import type { StepKind } from "./types";

/** Approximate picker dimensions used for viewport-overflow math. The
 * picker has `w-[260px]` and renders 3 rows of 2 chips ≈ ~210px tall. */
export const KIND_PICKER_WIDTH = 260;
export const KIND_PICKER_HEIGHT = 220;

interface KindPickerPopoverProps {
  anchorRef: RefObject<HTMLElement | null>;
  onPick: (kind: StepKind) => void;
  onClose: () => void;
}

export function KindPickerPopover({ anchorRef, onPick, onClose }: KindPickerPopoverProps) {
  const coords = usePopoverAnchor(anchorRef, {
    width: KIND_PICKER_WIDTH,
    height: KIND_PICKER_HEIGHT,
  });

  // SSR guard — createPortal requires a real DOM node.
  if (typeof document === "undefined") return null;
  if (!coords) return null;

  return createPortal(
    <div
      className="fixed inset-0 z-50"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) onClose();
      }}
    >
      <div
        style={{ position: "fixed", top: coords.top, left: coords.left, width: KIND_PICKER_WIDTH }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <KindPicker
          onPick={(kind) => {
            onPick(kind);
            onClose();
          }}
          onClose={onClose}
        />
      </div>
    </div>,
    document.body,
  );
}
