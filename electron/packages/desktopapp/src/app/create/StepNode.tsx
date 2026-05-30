"use client";

/**
 * StepNode
 *
 * Custom reactflow node component used for every step on the editor
 * canvas. Renders:
 *   - a small persona-mesh badge with the kind icon
 *   - the step id (mono)
 *   - a one-line `summarize()` of the step's payload
 *   - branch-label eyebrow (when the node is the entry of a match case)
 *   - hover-revealed quick actions (kind switch, edit, delete)
 *   - hover-revealed drag-handle grip on the top-left
 *   - small left/right connection handles styled as grabbable dots
 *     (hover-revealed) used for edge reconnect — the actual gesture is
 *     handled by reactflow via `onReconnect` in `WorkflowGraphEditor`.
 *
 * Keyboard reorder (up/down arrows after grip-focus) moves the step in
 * the underlying `steps[]` array — this remains as an accessibility
 * affordance alongside the new free drag + edge-reconnect gestures.
 */

import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { Node } from "@xyflow/react";
import { GripVertical } from "lucide-react";
import { STEP_KIND_META } from "./stepMeta";
import type { StepNodeData } from "./graph";
import { HoverActions } from "./HoverActions";

type StepNodeType = Node<StepNodeData, "step">;

export function StepNode({ data, selected }: NodeProps<StepNodeType>) {
  const meta = STEP_KIND_META[data.step.kind];
  const Icon = meta.Icon;
  const canDelete = data.canDelete ?? true;

  return (
    <div
      className={[
        "group relative rounded-card border bg-surface overflow-visible shadow-sm transition-shadow w-[240px] cursor-pointer hover:shadow-menu",
        selected
          ? "border-brand ring-2 ring-brand-ring"
          : "border-border hover:border-border-strong",
      ].join(" ")}
    >
      {/* Handles — small dots, brightened on group hover so the user
          knows they can grab an edge end. Tokens only. */}
      <Handle
        type="target"
        position={Position.Left}
        className="!w-[10px] !h-[10px] !bg-border group-hover:!bg-brand !border-2 !border-surface transition-colors cursor-grab"
      />

      {/* Drag-handle grip — visible on hover. Focus + arrow keys reorder. */}
      {/* TODO: implement true drag-and-drop reorder; for now, this is a keyboard affordance. */}
      <button
        type="button"
        tabIndex={0}
        onKeyDown={(e) => {
          if (e.key === "ArrowUp" || e.key === "ArrowDown") {
            e.preventDefault();
            data.onReorder?.(data.path, e.key === "ArrowUp" ? "up" : "down");
          }
        }}
        className="absolute top-1 left-1 hidden group-hover:flex focus:flex items-center justify-center h-4 w-4 text-fg-subtle hover:text-fg cursor-grab active:cursor-grabbing outline-none focus-visible:ring-2 focus-visible:ring-brand-ring rounded"
        aria-label="Reorder step (arrow keys when focused)"
        title="Reorder (focus + ↑/↓)"
      >
        <GripVertical size={11} />
      </button>

      {/* Hover quick actions */}
      <HoverActions
        KindIcon={Icon}
        onSwitchKind={() => data.onSwitchKind?.(data.path)}
        onEdit={() => data.onEdit?.(data.path)}
        onDelete={() => data.onDelete?.(data.path)}
        canDelete={canDelete}
      />

      <div data-mesh={meta.mesh} className="rounded-t-card px-3 py-2 flex items-center gap-2">
        <span className="inline-flex h-6 w-6 items-center justify-center rounded-pill bg-surface/80 text-fg">
          <Icon size={13} strokeWidth={2.25} />
        </span>
        <div className="flex-1 min-w-0">
          <div className="text-[10px] font-mono tracking-wider uppercase text-fg-secondary">
            {meta.label}
          </div>
          {data.branchLabel && (
            <div className="text-[9px] font-mono uppercase tracking-wider text-fg-tertiary truncate">
              branch · {data.branchLabel}
            </div>
          )}
        </div>
      </div>
      <div className="px-3 py-2 border-t border-border-subtle">
        <div className="text-[11px] font-mono text-fg truncate">{data.step.id}</div>
        <div className="text-[12px] text-fg-secondary mt-1 line-clamp-2 break-words">
          {data.summary}
        </div>
      </div>
      <Handle
        type="source"
        position={Position.Right}
        className="!w-[10px] !h-[10px] !bg-border group-hover:!bg-brand !border-2 !border-surface transition-colors cursor-grab"
      />
    </div>
  );
}
