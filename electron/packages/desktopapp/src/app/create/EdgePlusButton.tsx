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
 * Portal: the kind picker popover is rendered via the shared
 * `KindPickerPopover` (extracted post-ACS-20 so the per-chain tail-+
 * button can share the same popover). It portals to `document.body`
 * to escape the reactflow canvas stacking context that would otherwise
 * drop the popover behind neighbouring nodes.
 */

import { useRef, useState } from "react";
import {
  BaseEdge,
  EdgeLabelRenderer,
  getSmoothStepPath,
  type EdgeProps,
} from "@xyflow/react";
import { Plus } from "lucide-react";
import { KindPickerPopover } from "./KindPickerPopover";
import type { StepKind } from "./types";

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
        <KindPickerPopover
          anchorRef={buttonRef}
          onPick={(kind) => insertData.onInsert(insertData.sourcePath, kind)}
          onClose={() => setPickerOpen(false)}
        />
      )}
    </>
  );
}
