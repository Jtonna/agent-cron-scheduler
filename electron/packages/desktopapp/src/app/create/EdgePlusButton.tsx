"use client";

/**
 * EdgePlusButton
 *
 * Tiny floating circular `+` button used as a custom reactflow edge.
 * Clicking it opens the kind picker; selecting a kind inserts a new
 * step between the two nodes the edge connects.
 *
 * Implemented as a custom reactflow edge so it inherits proper path
 * routing; the button sits at the edge midpoint via a Foreignobject.
 *
 * Portal: the kind picker popover is rendered via `createPortal` into
 * `document.body` and positioned via `usePopoverAnchor` (with a
 * flip-above when the picker would overflow the viewport). Without this,
 * the popover lives inside the reactflow EdgeLabelRenderer — whose
 * transformed canvas stacking context drops the popover behind
 * neighbouring nodes. Same family of fix as the StepEditorModal portal
 * (commit ebc820a).
 */

import { useRef, useState } from "react";
import { createPortal } from "react-dom";
import {
  BaseEdge,
  EdgeLabelRenderer,
  getSmoothStepPath,
  type EdgeProps,
} from "@xyflow/react";
import { Plus } from "lucide-react";
import { KindPicker } from "./KindPicker";
import { usePopoverAnchor } from "./usePopoverAnchor";
import type { StepKind } from "./types";

/** Approximate picker dimensions used for viewport-overflow math. The
 * picker has `w-[260px]` and renders 3 rows of 2 chips ≈ ~210px tall. */
const PICKER_WIDTH = 260;
const PICKER_HEIGHT = 220;

export interface InsertEdgeData extends Record<string, unknown> {
  /** Path of the source step (where insertion happens after). */
  sourcePath: string;
  onInsert: (sourcePath: string, kind: StepKind) => void;
}

export function InsertEdge(props: EdgeProps) {
  const { id, sourceX, sourceY, targetX, targetY, sourcePosition, targetPosition, markerEnd, style, label, data } =
    props;
  const insertData = data as InsertEdgeData | undefined;

  const [edgePath, labelX, labelY] = getSmoothStepPath({
    sourceX,
    sourceY,
    sourcePosition,
    targetX,
    targetY,
    targetPosition,
  });
  const buttonRef = useRef<HTMLButtonElement>(null);
  const [pickerOpen, setPickerOpen] = useState(false);

  return (
    <>
      <BaseEdge id={id} path={edgePath} markerEnd={markerEnd} style={style} />
      <EdgeLabelRenderer>
        <div
          style={{
            position: "absolute",
            transform: `translate(-50%, -50%) translate(${labelX}px, ${labelY}px)`,
            pointerEvents: "all",
          }}
          className="group"
        >
          {label && (
            <span className="block text-[10px] font-mono text-fg-muted bg-surface px-1.5 py-0.5 rounded-pill border border-border mb-1 whitespace-nowrap">
              {label}
            </span>
          )}
          <button
            ref={buttonRef}
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setPickerOpen((v) => !v);
            }}
            className="opacity-0 group-hover:opacity-100 focus:opacity-100 transition-opacity inline-flex items-center justify-center h-[22px] w-[22px] rounded-full bg-surface border-2 border-dashed border-brand text-brand hover:bg-brand hover:text-surface hover:border-solid cursor-pointer shadow-sm"
            aria-label="Insert step here"
            title="Insert step"
          >
            <Plus size={11} strokeWidth={2.5} />
          </button>
        </div>
      </EdgeLabelRenderer>
      {pickerOpen && insertData && (
        <PortalledKindPicker
          anchorRef={buttonRef}
          onPick={(kind) => {
            insertData.onInsert(insertData.sourcePath, kind);
            setPickerOpen(false);
          }}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </>
  );
}

interface PortalledKindPickerProps {
  anchorRef: React.RefObject<HTMLButtonElement | null>;
  onPick: (kind: StepKind) => void;
  onClose: () => void;
}

/**
 * Renders the `KindPicker` into `document.body` so it escapes the
 * reactflow canvas stacking context. Position is computed via
 * `usePopoverAnchor`, which centres on the trigger, clamps to the
 * viewport, and flips above when there isn't room below.
 */
function PortalledKindPicker({ anchorRef, onPick, onClose }: PortalledKindPickerProps) {
  const coords = usePopoverAnchor(anchorRef, {
    width: PICKER_WIDTH,
    height: PICKER_HEIGHT,
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
        style={{ position: "fixed", top: coords.top, left: coords.left, width: PICKER_WIDTH }}
        onMouseDown={(e) => e.stopPropagation()}
      >
        <KindPicker onPick={onPick} onClose={onClose} />
      </div>
    </div>,
    document.body,
  );
}
