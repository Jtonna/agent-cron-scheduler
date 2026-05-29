"use client";

/**
 * KindPicker
 *
 * Compact grid picker for selecting which kind of step to insert next.
 * Used both by the workflow toolbar's "Add step" button and by the per-
 * node "insert after" affordance.
 */

import { STEP_KIND_META, STEP_KINDS } from "./stepMeta";
import type { StepKind } from "./types";

interface KindPickerProps {
  onPick: (kind: StepKind) => void;
  onClose?: () => void;
}

export function KindPicker({ onPick, onClose }: KindPickerProps) {
  return (
    <div
      className="bg-surface border border-border rounded-card shadow-menu p-3 w-[260px]"
      role="menu"
    >
      <div className="text-eyebrow mb-2">Add step</div>
      <div className="grid grid-cols-2 gap-2">
        {STEP_KINDS.map((kind) => {
          const meta = STEP_KIND_META[kind];
          const Icon = meta.Icon;
          return (
            <button
              key={kind}
              type="button"
              onClick={() => {
                onPick(kind);
                onClose?.();
              }}
              className="flex items-start gap-2 p-2 rounded-input border border-border hover:border-fg hover:bg-surface-hover text-left cursor-pointer"
            >
              <span
                data-mesh={meta.mesh}
                className="inline-flex h-6 w-6 rounded-pill items-center justify-center shrink-0"
              >
                <Icon size={12} className="text-fg" strokeWidth={2.25} />
              </span>
              <div className="min-w-0">
                <div className="text-xs font-semibold text-fg truncate">{meta.label}</div>
                <div className="text-[10px] text-fg-muted truncate">{meta.description}</div>
              </div>
            </button>
          );
        })}
      </div>
    </div>
  );
}
