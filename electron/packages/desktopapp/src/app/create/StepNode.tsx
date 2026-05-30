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
 *   - small left/right connection handles styled as visible dots
 *     (always rendered, brightened on hover) used for edge wiring —
 *     the actual gesture is handled by reactflow via `onConnect` /
 *     `onReconnect` in `WorkflowGraphEditor`.
 *
 * Handle visibility (n8n-style, post-ACS-20):
 *   - Idle: small `bg-fg-muted` dot, always visible so the user knows
 *     the affordance exists at rest.
 *   - Hover: brightens to `bg-fg`.
 *   - In-flight connection: reactflow swaps in its own internal
 *     "connecting" state class; we style that via a `:hover`-equivalent
 *     selector with the brand accent.
 *
 * Disconnected state:
 *   - When `data.disconnected` is true (the node was added via the
 *     dock and not yet wired into the chain), the outer border switches
 *     to a dashed brand-coloured stroke and a small "Not wired" badge
 *     appears in the top-right corner. The badge clears as soon as the
 *     user drags a connection into this node.
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
  const disconnected = data.disconnected === true;

  const borderClass = selected
    ? "border-brand ring-2 ring-brand-ring"
    : disconnected
      ? "border-2 border-dashed border-brand/60 hover:border-brand"
      : "border border-border hover:border-border-strong";

  return (
    <div
      className={[
        "group relative rounded-card bg-surface overflow-visible shadow-sm transition-shadow w-[240px] cursor-pointer hover:shadow-menu",
        borderClass,
      ].join(" ")}
    >
      {/* Handles — small dots, visible at rest in a muted tone so the
          user can find them, brightened on group hover, and accented
          when a connection is in flight (via reactflow's `.connecting`
          internal class). Tokens only. */}
      <Handle
        type="target"
        position={Position.Left}
        className="!w-[10px] !h-[10px] !bg-fg-muted group-hover:!bg-fg !border-2 !border-surface transition-colors cursor-crosshair"
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

      {/* Disconnected affordance — small badge along the bottom edge of
          the card so it doesn't fight the hover actions for the top
          corners. Tokens only. */}
      {disconnected && (
        <div
          className="absolute -bottom-2 left-1/2 -translate-x-1/2 z-10 px-1.5 py-0.5 rounded-pill bg-surface border border-brand text-brand text-[9px] font-mono uppercase tracking-wider shadow-sm whitespace-nowrap pointer-events-none"
          title="This step is not wired into the chain — drag from a handle to connect"
        >
          not wired
        </div>
      )}

      <Handle
        type="source"
        position={Position.Right}
        className="!w-[10px] !h-[10px] !bg-fg-muted group-hover:!bg-fg !border-2 !border-surface transition-colors cursor-crosshair"
      />
    </div>
  );
}
