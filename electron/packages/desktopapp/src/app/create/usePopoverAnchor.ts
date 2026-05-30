"use client";

/**
 * usePopoverAnchor
 *
 * Computes a viewport-anchored position for a portalled popover/menu
 * relative to a trigger element. Designed for the small "open a picker
 * near this button, escape any local stacking context by portalling to
 * `document.body`" pattern used inside the workflow graph editor (see
 * `EdgePlusButton` / `PortalledKindPicker`).
 *
 * Behaviour:
 *   - Centres the popover horizontally on the trigger, then clamps it
 *     to the viewport with an 8px margin.
 *   - Tries to place below the trigger; flips above when there isn't
 *     enough room.
 *   - Returns `null` until the layout effect has measured the trigger,
 *     so callers can simply early-return that frame to avoid flashes.
 *
 * Not promoted to a global shared hook because (a) it's tuned to the
 * graph editor's specific 8px margin and gap conventions and (b) the
 * desktop app's other popover surfaces use react-aria's overlay
 * primitives. If a second editor-internal use shows up, this is the
 * place to standardise.
 */

import { useLayoutEffect, useState, type RefObject } from "react";

export interface PopoverAnchorCoords {
  top: number;
  left: number;
}

export interface UsePopoverAnchorOptions {
  /** Expected width of the popover, used for horizontal clamping. */
  width: number;
  /** Expected height of the popover, used to decide flip-above. */
  height: number;
  /** Gap in pixels between the trigger and the popover. */
  gap?: number;
  /** Viewport margin enforced when clamping. */
  viewportMargin?: number;
}

export function usePopoverAnchor(
  anchorRef: RefObject<HTMLElement | null>,
  { width, height, gap = 6, viewportMargin = 8 }: UsePopoverAnchorOptions,
): PopoverAnchorCoords | null {
  const [coords, setCoords] = useState<PopoverAnchorCoords | null>(null);

  useLayoutEffect(() => {
    const el = anchorRef.current;
    if (!el) return;
    const rect = el.getBoundingClientRect();
    const viewportH = window.innerHeight;
    const viewportW = window.innerWidth;
    const fitsBelow = rect.bottom + gap + height <= viewportH;
    const top = fitsBelow
      ? rect.bottom + gap
      : Math.max(viewportMargin, rect.top - gap - height);
    const rawLeft = rect.left + rect.width / 2 - width / 2;
    const left = Math.max(
      viewportMargin,
      Math.min(rawLeft, viewportW - width - viewportMargin),
    );
    setCoords({ top, left });
  }, [anchorRef, width, height, gap, viewportMargin]);

  return coords;
}
