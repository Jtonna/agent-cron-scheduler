"use client";

import Link from "next/link";
import { ChevronLeft } from "lucide-react";

/**
 * SidebarBackLink
 *
 * The "back to ..." link used at the top of nested sidebars
 * (JobDetailSidebar and RunDetailSidebar). Both previously rolled their
 * own markup — one with a Next `Link`, the other with the `Button`
 * primitive with `intent="ghost"` and `shape="rounded"`. The visual
 * result was close but not identical (different padding, different hover
 * surface).
 *
 * Single markup now: small chevron + label, ghost intent, left-aligned,
 * `rounded-input` shape, `text-xs` density. Truncates long destination
 * labels with `truncate`.
 */

export interface SidebarBackLinkProps {
  href: string;
  /** Visible label, e.g. "Workflows" or the parent workflow name. */
  children: React.ReactNode;
}

export function SidebarBackLink({ href, children }: SidebarBackLinkProps) {
  return (
    <Link
      href={href}
      className="inline-flex items-center gap-1.5 px-2 py-1 rounded-input text-xs text-fg-muted hover:text-fg hover:bg-surface-hover transition-colors cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring max-w-full"
    >
      <ChevronLeft size={14} className="shrink-0" />
      <span className="truncate">{children}</span>
    </Link>
  );
}
