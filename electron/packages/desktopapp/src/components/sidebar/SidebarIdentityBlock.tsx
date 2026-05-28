"use client";

import type { ReactNode } from "react";

/**
 * SidebarIdentityBlock
 *
 * The "who is this sidebar about" block — workflow name + a meta line
 * (cron, timezone, run id, ...) on the left, with a right-aligned actions
 * slot for things like the favorite star. Both JobDetailSidebar and
 * RunDetailSidebar had their own inline version with subtly different
 * padding and border treatment; this consolidates them into a single
 * primitive.
 *
 * Visual contract:
 *  - `px-2 py-3 border-b border-border-subtle` — same on both rails.
 *  - Title: `text-base font-semibold text-fg truncate`.
 *  - Meta:  `text-xs text-fg-muted truncate mt-0.5`, monospaced when
 *           `monoMeta` is true (cron / id surfaces).
 *  - Actions sit on the right, top-aligned with the title row.
 */

export interface SidebarIdentityBlockProps {
  title: string;
  meta?: string;
  monoMeta?: boolean;
  /** Right-aligned actions, e.g. a favorite star. */
  actions?: ReactNode;
  /** Optional tooltip override for the title. Defaults to `title`. */
  titleTooltip?: string;
  /** Optional tooltip override for the meta line. Defaults to `meta`. */
  metaTooltip?: string;
}

export function SidebarIdentityBlock({
  title,
  meta,
  monoMeta = false,
  actions,
  titleTooltip,
  metaTooltip,
}: SidebarIdentityBlockProps) {
  return (
    <div className="px-2 py-3 border-b border-border-subtle">
      <div className="flex items-start gap-1">
        <div className="flex-1 min-w-0">
          <div
            className="text-base font-semibold text-fg truncate"
            title={titleTooltip ?? title}
          >
            {title}
          </div>
          {meta && (
            <div
              className={[
                "text-xs text-fg-muted truncate mt-0.5",
                monoMeta ? "font-mono" : "",
              ]
                .filter(Boolean)
                .join(" ")}
              title={metaTooltip ?? meta}
            >
              {meta}
            </div>
          )}
        </div>
        {actions && <div className="shrink-0">{actions}</div>}
      </div>
    </div>
  );
}
