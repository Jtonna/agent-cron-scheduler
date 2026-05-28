"use client";

import type { ReactNode } from "react";

/**
 * SidebarSectionHeader
 *
 * Unified eyebrow header used by every section across all three
 * sidebars — STATUS, RECENT RUNS, FAVORITED, RECENT, STEPS. Same font
 * weight (semibold), tracking (wider), color (`text-fg-subtle`),
 * size (`text-[11px]`), and padding (`px-2`) everywhere. Optional
 * trailing meta slot for "(6)", "5 of 7 ran", etc.
 *
 * Replaces:
 *  - `SidebarSectionHeading` (used by JobsSidebar via SidebarSection)
 *  - the inline eyebrow `div` in JobDetailSidebar's STATUS + RECENT RUNS
 *  - the inline eyebrow `div` in RunDetailSidebar's STEPS row
 *
 * Vertical rhythm is owned by the parent container's `gap-*` — this
 * component does not set its own top/bottom margin.
 */

export interface SidebarSectionHeaderProps {
  title: string;
  /** Optional right-aligned meta, e.g. "(6)" or "5 of 7 ran". */
  meta?: ReactNode;
}

export function SidebarSectionHeader({
  title,
  meta,
}: SidebarSectionHeaderProps) {
  return (
    <div className="px-2 flex items-center justify-between">
      <span className="text-[11px] font-semibold text-fg-subtle uppercase tracking-wider">
        {title}
      </span>
      {meta !== undefined && meta !== null && (
        <span className="text-[11px] text-fg-muted">{meta}</span>
      )}
    </div>
  );
}
