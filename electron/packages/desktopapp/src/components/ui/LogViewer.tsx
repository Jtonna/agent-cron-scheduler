"use client";

import dynamic from "next/dynamic";
import type { ReactNode } from "react";

/**
 * LogViewer
 *
 * Cross-cutting wrapper around `@melloware/react-logviewer`'s `LazyLog`.
 * Used wherever the app needs to display a streaming or static log body
 * (currently: the System Logs page and the run-detail page).
 *
 * Quirks worth knowing:
 * - `LazyLog` crashes if `text` is an empty string, so this component
 *   internally swaps "" for a single space (" ") before passing it down.
 * - The component is loaded via `next/dynamic` with `ssr: false` because
 *   `LazyLog` reaches for browser-only APIs at import time.
 * - All visual styling flows through our design tokens via CSS variables
 *   (`--color-surface`, `--color-fg`, `--font-geist-mono`) — no raw colors.
 * - The wrapping `<div>` has `h-full w-full relative` so callers must give
 *   it a sized parent; pass extra layout classes via `className`.
 *
 * Optional props:
 * - `scrollToLine` — passed through to `LazyLog` to jump to a specific
 *   line (1-based). Used by the run-detail page when a step is selected
 *   in the sidebar.
 * - `actions` — when provided, renders a thin top bar above the log area
 *   with the slot anchored to the right (used for the kill-run button on
 *   in-flight runs). When omitted, no bar is rendered (default behavior).
 */

const LazyLog = dynamic(() => import("@melloware/react-logviewer").then((mod) => mod.LazyLog), {
  ssr: false,
});

export interface LogViewerProps {
  /** The log body to render. Empty strings are replaced with a single space so LazyLog doesn't crash. */
  text: string;
  /** When true, auto-scrolls to the bottom as new lines arrive. Defaults to `true`. */
  follow?: boolean;
  /** Additional classes applied to the wrapping container (which is `h-full w-full relative` by default). */
  className?: string;
  /** Optional 1-based line number to scroll to (passed through to LazyLog). */
  scrollToLine?: number;
  /** Optional slot for action buttons. When provided, a thin top bar is rendered above the log. */
  actions?: ReactNode;
}

export function LogViewer({
  text,
  follow = true,
  className,
  scrollToLine,
  actions,
}: LogViewerProps) {
  // LazyLog throws on an empty string; substitute a single space so the
  // viewer renders an empty pane instead of crashing.
  const safeText = text || " ";

  return (
    <div className={`h-full w-full relative flex flex-col ${className ?? ""}`}>
      {actions !== undefined && actions !== null && (
        <div className="h-9 px-3 flex items-center justify-end border-b border-border-subtle bg-surface-secondary shrink-0">
          {actions}
        </div>
      )}
      <div className="relative flex-1 min-h-0">
        <LazyLog
          text={safeText}
          follow={follow}
          scrollToLine={scrollToLine}
          enableSearch
          enableHotKeys
          selectableLines
          enableLineNumbers
          extraLines={1}
          height="auto"
          style={{
            background: "var(--color-surface)",
            color: "var(--color-fg)",
            fontFamily: "var(--font-geist-mono), monospace",
            fontSize: "13px",
          }}
          containerStyle={{
            overflow: "auto",
          }}
        />
      </div>
    </div>
  );
}
