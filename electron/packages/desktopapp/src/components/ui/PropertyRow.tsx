"use client";

/**
 * PropertyRow
 *
 * A compact label/value row used by the sidebar STATUS section (and
 * other property-listing surfaces). Renders a left-aligned, muted label
 * and a right-aligned value. Value can be plain text, mono text (for
 * cron expressions / IDs), or any ReactNode (for inline toggles, pills,
 * etc.). Designed to compose into a tight vertical stack — no internal
 * margins, the parent owns the rhythm.
 *
 * Lives in `ui/` because it has no domain knowledge — could be lifted
 * unchanged into any other app surface that needs key/value rows.
 */

import type { ReactNode } from "react";

export interface PropertyRowProps {
  label: string;
  /**
   * Plain string value (rendered as right-aligned text), or a custom
   * ReactNode (toggle, pill, etc.). When `value` is a string and `mono`
   * is true, it renders in a monospace face.
   */
  value?: ReactNode;
  /** Render the string value in mono. Ignored when `value` is a ReactNode. */
  mono?: boolean;
  /** Optional tooltip when the value is truncated. */
  title?: string;
}

export function PropertyRow({ label, value, mono, title }: PropertyRowProps) {
  const isStringValue = typeof value === "string";
  const renderedValue = isStringValue ? (
    <span
      className={[
        "text-xs text-fg truncate text-right",
        mono ? "font-mono" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      title={title ?? (value as string)}
    >
      {value || "—"}
    </span>
  ) : (
    value ?? <span className="text-xs text-fg-muted">—</span>
  );

  return (
    <div className="flex items-center justify-between gap-3 px-2 py-1.5 min-h-[28px]">
      <span className="text-xs text-fg-muted shrink-0">{label}</span>
      <div className="min-w-0 flex items-center justify-end">{renderedValue}</div>
    </div>
  );
}
