"use client";

/**
 * HoverActions
 *
 * Small floating action row above a step node. Shows on node hover and
 * exposes quick actions:
 *   - kind icon → opens the kind-switcher (delegated to parent)
 *   - pencil → opens the step editor modal
 *   - trash → deletes the step
 *
 * Rendered inside the step node component so it inherits hover state
 * via group hover.
 */

import { Pencil, Trash2 } from "lucide-react";
import type { ComponentType } from "react";
import type { LucideProps } from "lucide-react";

interface HoverActionsProps {
  KindIcon: ComponentType<LucideProps>;
  onSwitchKind: () => void;
  onEdit: () => void;
  onDelete: () => void;
  canDelete: boolean;
}

export function HoverActions({
  KindIcon,
  onSwitchKind,
  onEdit,
  onDelete,
  canDelete,
}: HoverActionsProps) {
  return (
    <div
      className="absolute -top-3 right-0 hidden group-hover:flex items-center gap-0.5 bg-surface border border-border rounded-pill shadow-menu p-0.5 z-10"
      role="toolbar"
      aria-label="Step actions"
    >
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onSwitchKind();
        }}
        className="inline-flex items-center justify-center h-5 w-5 rounded-full hover:bg-surface-hover text-fg-muted hover:text-fg cursor-pointer"
        aria-label="Switch kind"
        title="Switch kind"
      >
        <KindIcon size={11} strokeWidth={2.25} />
      </button>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          onEdit();
        }}
        className="inline-flex items-center justify-center h-5 w-5 rounded-full hover:bg-surface-hover text-fg-muted hover:text-fg cursor-pointer"
        aria-label="Edit step"
        title="Edit"
      >
        <Pencil size={11} strokeWidth={2.25} />
      </button>
      <button
        type="button"
        onClick={(e) => {
          e.stopPropagation();
          if (canDelete) onDelete();
        }}
        disabled={!canDelete}
        className={[
          "inline-flex items-center justify-center h-5 w-5 rounded-full cursor-pointer",
          canDelete
            ? "text-fg-muted hover:text-status-failed hover:bg-status-failed-bg"
            : "text-fg-faint cursor-not-allowed",
        ].join(" ")}
        aria-label="Delete step"
        title={canDelete ? "Delete" : "Cannot delete the last remaining step"}
      >
        <Trash2 size={11} strokeWidth={2.25} />
      </button>
    </div>
  );
}
