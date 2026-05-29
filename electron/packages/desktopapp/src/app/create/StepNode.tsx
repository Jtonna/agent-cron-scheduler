"use client";

/**
 * StepNode
 *
 * Custom reactflow node component used for every step on the /create
 * graph. Renders:
 *   - a small persona-mesh badge with the kind icon
 *   - the step id (mono)
 *   - a one-line `summarize()` of the step's payload
 *   - branch-label eyebrow (when the node is the entry of a match case)
 *
 * Click selection is handled by reactflow itself (`onNodeClick` on the
 * parent canvas); this component just paints the chrome.
 */

import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { Node } from "@xyflow/react";
import { STEP_KIND_META } from "./stepMeta";
import type { StepNodeData } from "./graph";

type StepNodeType = Node<StepNodeData, "step">;

export function StepNode({ data, selected }: NodeProps<StepNodeType>) {
  const meta = STEP_KIND_META[data.step.kind];
  const Icon = meta.Icon;

  return (
    <div
      className={[
        "rounded-card border bg-surface overflow-hidden shadow-sm transition-shadow w-[240px]",
        selected ? "border-fg ring-2 ring-brand-ring" : "border-border hover:border-border-strong",
      ].join(" ")}
    >
      <Handle
        type="target"
        position={Position.Left}
        className="!bg-fg-subtle !border-surface"
      />
      <div
        data-mesh={meta.mesh}
        className="px-3 py-2 flex items-center gap-2"
      >
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
        className="!bg-fg-subtle !border-surface"
      />
    </div>
  );
}
