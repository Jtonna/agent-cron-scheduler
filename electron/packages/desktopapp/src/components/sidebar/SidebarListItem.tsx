"use client";

import type { ReactNode } from "react";
import Link from "next/link";
import { Button as AriaButton } from "react-aria-components";
import {
  JobStateIndicator,
  type JobState,
} from "@/components/ui/JobStateIndicator";

/**
 * SidebarListItem
 *
 * Unified row used by the list sections of JobDetailSidebar (Recent
 * Runs) and RunDetailSidebar (Steps). Before this primitive existed
 * each sidebar rendered its own near-identical row with subtly
 * different padding, hover surface, and dot size.
 *
 * Layout:
 *
 *     [ • ]  title (truncates, mono)        meta · meta
 *
 *   - `state` controls the leading status dot (via JobStateIndicator).
 *   - `title` is the primary identifier (run id, step id, …) and is
 *     rendered mono.
 *   - Trailing metas (`meta` and optional `metaSecondary`) right-align.
 *     The secondary meta is mono — used for cost / numeric trailers.
 *   - Either `href` (Next.js Link) or `onPress` (button) makes the row
 *     interactive; both apply the same focus ring and hover surface.
 *   - `active` swaps the hover surface for a sticky-selected look with a
 *     brand left-border (used by the steps list).
 */

export interface SidebarListItemProps {
  state: JobState;
  title: string;
  /** Optional tooltip; defaults to `title`. */
  titleTooltip?: string;
  meta?: ReactNode;
  metaSecondary?: ReactNode;
  active?: boolean;
  href?: string;
  onPress?: () => void;
  ariaLabel?: string;
}

const ROW_BASE =
  "w-full flex items-center gap-2 px-2 py-1.5 rounded-input text-xs text-left transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring border-l-2";

function rowClasses(active: boolean): string {
  return active
    ? `${ROW_BASE} border-brand bg-surface-secondary`
    : `${ROW_BASE} border-transparent hover:bg-surface-hover`;
}

export function SidebarListItem({
  state,
  title,
  titleTooltip,
  meta,
  metaSecondary,
  active = false,
  href,
  onPress,
  ariaLabel,
}: SidebarListItemProps) {
  const content = (
    <>
      <JobStateIndicator state={state} variant="dot" size="sm" />
      <span
        className="font-mono text-fg truncate flex-1"
        title={titleTooltip ?? title}
      >
        {title}
      </span>
      {meta !== undefined && meta !== null && (
        <span className="text-fg-muted whitespace-nowrap text-[11px]">
          {meta}
        </span>
      )}
      {metaSecondary !== undefined && metaSecondary !== null && (
        <span className="font-mono text-fg-muted whitespace-nowrap text-[11px]">
          {metaSecondary}
        </span>
      )}
    </>
  );

  const cls = rowClasses(active);

  if (href) {
    return (
      <Link href={href} className={cls} aria-label={ariaLabel}>
        {content}
      </Link>
    );
  }

  if (onPress) {
    return (
      <AriaButton
        type="button"
        onPress={onPress}
        className={cls}
        aria-label={ariaLabel}
        aria-pressed={active}
      >
        {content}
      </AriaButton>
    );
  }

  return <span className={cls}>{content}</span>;
}
