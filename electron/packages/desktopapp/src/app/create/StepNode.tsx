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
 *   - left/right connection handles used for edge wiring — the actual
 *     gesture is handled by reactflow via `onConnect` / `onReconnect`
 *     in `WorkflowGraphEditor`.
 *
 * Interaction model (post-ACS-20, n8n-style):
 *   - SINGLE click on the node body → selects the node (visible brand
 *     ring via reactflow's built-in `selected` prop). Does NOT open the
 *     editor — single-click had been swallowing reach-for-the-handle
 *     gestures, which is the same trap n8n's editor explicitly avoids.
 *   - DOUBLE click on the node body → opens the step editor modal.
 *     Wired in `WorkflowGraphEditor` via `onNodeDoubleClick`. This
 *     matches n8n's `CanvasNodeDefault.vue` (`@dblclick.stop="onActivate"`).
 *   - Pencil icon in the hover-actions row still opens the modal as a
 *     one-click fallback affordance for mouse-only users.
 *   - The card body's cursor stays as the high-contrast pointer (no
 *     pointer/edit-affordance change on hover) because the primary
 *     editor-open gesture is now double-click, not single-click. The
 *     hover treatment is the actions row + border darken.
 *
 * Handle geometry (n8n-style, post-ACS-20 UX rework):
 *
 *   We mirror n8n's `CanvasHandleDot.vue` / `CanvasHandleRenderer.vue`
 *   pattern — handles are a small visible dot wrapped in a generous
 *   transparent hit zone that PROTRUDES outward from the card edge,
 *   so the dot reads as "this is a connection point, grab me" rather
 *   than blending into the card chrome.
 *
 *   - The reactflow `<Handle>` element is a 24×24 transparent square
 *     (n8n uses ~16px dot + 4px padding → 24px total hit zone). We
 *     reproduce that with `!w-6 !h-6` and force a transparent
 *     background so no stray reactflow chrome bleeds through.
 *   - The handle is centred on the card edge (reactflow's default for
 *     left/right positions), so HALF of the 24px hit zone protrudes
 *     outward beyond the card border — that's the visual "stub" cue
 *     and also doubles the forgiveness margin for an approaching
 *     cursor (you can overshoot the card and still land in the zone).
 *   - Inside the zone we render an 8px visible dot, `pointer-events:
 *     none` so it never absorbs the grab. The dot is filled with the
 *     surface token + a coloured ring, so it visually pops against
 *     both the card and the canvas background.
 *   - On hover (group or handle-local) the dot scales 1.25× and the
 *     ring darkens — matches the n8n "border thickens + scale(1.5)"
 *     idea but tuned slightly subtler for our denser node card.
 *   - Cursor: source (right) handle uses `cursor-crosshair` like n8n's
 *     output; target (left) handle uses the default pointer because
 *     you don't *start* a connection from a target — you drop onto
 *     one. The `[data-acs-editor]` CSS still applies our high-contrast
 *     SVG cursor variants.
 *   - In-flight connection styling is handled at the ReactFlow level
 *     via `connectionLineStyle`.
 *
 * Disconnected state:
 *   - When `data.disconnected` is true (the node was added via the
 *     dock and not yet wired into the chain), the outer border switches
 *     to a dashed brand-coloured stroke and a small "Not wired" badge
 *     appears along the bottom edge. The badge clears as soon as the
 *     user drags a connection into this node. The target handle stays
 *     present and active on disconnected nodes — that's how the user
 *     wires them in.
 *
 * Keyboard reorder (up/down arrows after grip-focus) moves the step in
 * the underlying `steps[]` array — this remains as an accessibility
 * affordance alongside the free drag + edge-reconnect gestures.
 */

import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { Node } from "@xyflow/react";
import { GripVertical } from "lucide-react";
import { STEP_KIND_META } from "./stepMeta";
import type { StepNodeData } from "./graph";
import { HoverActions } from "./HoverActions";

type StepNodeType = Node<StepNodeData, "step">;

/**
 * Shared handle hit-zone classes. 24px transparent square — large
 * enough to forgive an imprecise cursor approach and to make the dot
 * visually protrude from the card (reactflow centres handles on the
 * positioned edge, so ~12px of the 24px zone sits OUTSIDE the card).
 * `!bg-transparent !border-0` strips reactflow's default dot styling
 * so our inner span fully owns the visual.
 */
const HANDLE_HIT_CLASSES =
  "!w-6 !h-6 !bg-transparent !border-0 flex items-center justify-center";

/**
 * The visible dot inside the handle hit zone. `pointer-events: none`
 * so it never competes with the parent <Handle> for the drag gesture.
 * Group-hover or local-hover bumps the scale + ring colour so the
 * connection-point affordance lights up when the user gets close.
 */
const HANDLE_DOT_CLASSES =
  "pointer-events-none block w-2 h-2 rounded-full bg-surface ring-2 ring-fg-muted " +
  "transition-transform transition-colors duration-150 " +
  "group-hover:ring-fg group-hover/handle:scale-125 group-hover/handle:ring-brand";

export function StepNode({ data, selected }: NodeProps<StepNodeType>) {
  const meta = STEP_KIND_META[data.step.kind];
  const Icon = meta.Icon;
  const canDelete = data.canDelete ?? true;
  const disconnected = data.disconnected === true;

  // Selection ring is the n8n analogue of `box-shadow: 0 0 0 6px
  // var(--canvas--color--selected-transparent)` — a clear-but-quiet
  // outline that doesn't fight the kind-mesh strip for attention.
  // Tokens only; ring-brand-ring is the project's standard
  // focus/selection halo.
  const borderClass = selected
    ? "border-brand ring-2 ring-brand-ring"
    : disconnected
      ? "border-2 border-dashed border-brand/60 hover:border-brand"
      : "border border-border hover:border-border-strong";

  return (
    <div
      className={[
        // No `cursor-pointer` on the body — single-click now selects
        // (reactflow built-in) and double-click opens the editor, so a
        // pointer cursor on the body would mislead the user into
        // thinking single-click would do something edit-ish. Default
        // SVG cursor from `cursors.ts` applies via the
        // `.react-flow__node` rule.
        "group relative rounded-card bg-surface overflow-visible shadow-sm transition-shadow w-[240px] hover:shadow-menu",
        borderClass,
      ].join(" ")}
    >
      {/* Target (input) handle — left edge, protrudes ~12px. Cursor
          stays as the default pointer because you don't START a
          connection from a target; you drop onto one. */}
      <div className="group/handle">
        <Handle
          type="target"
          position={Position.Left}
          className={HANDLE_HIT_CLASSES}
          style={{ pointerEvents: "all" }}
        >
          <span className={HANDLE_DOT_CLASSES} />
        </Handle>
      </div>

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

      {/* Hover quick actions — pencil still opens the modal in one
          click, which is the mouse-only fallback for users who prefer
          not to double-click. */}
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

      {/* Source (output) handle — right edge, protrudes ~12px.
          `cursor-crosshair` matches n8n's output-handle treatment so
          the user knows this is where you START dragging a wire. */}
      <div className="group/handle">
        <Handle
          type="source"
          position={Position.Right}
          className={`${HANDLE_HIT_CLASSES} cursor-crosshair`}
          style={{ pointerEvents: "all" }}
        >
          <span className={HANDLE_DOT_CLASSES} />
        </Handle>
      </div>
    </div>
  );
}
