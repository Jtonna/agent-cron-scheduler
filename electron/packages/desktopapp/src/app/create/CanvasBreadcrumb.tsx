"use client";

/**
 * CanvasBreadcrumb
 *
 * Tiny floating breadcrumb chip rendered between the Navbar and the
 * editor canvas. Stand-in for hosting a breadcrumb inside Navbar
 * itself (which currently doesn't accept children). When the Navbar
 * grows a `children` slot, this can be inlined there.
 */

import Link from "next/link";
import { ChevronRight } from "lucide-react";

export interface BreadcrumbItem {
  label: string;
  href?: string;
}

interface CanvasBreadcrumbProps {
  crumbs: BreadcrumbItem[];
}

export function CanvasBreadcrumb({ crumbs }: CanvasBreadcrumbProps) {
  return (
    <div className="border-b border-border-subtle bg-surface px-8 py-1.5">
      <nav aria-label="Breadcrumb" className="flex items-center gap-1 text-[11.5px] text-fg-muted">
        {crumbs.map((c, i) => (
          <span key={`${c.label}-${i}`} className="inline-flex items-center gap-1">
            {i > 0 && <ChevronRight size={11} className="text-fg-subtle" aria-hidden />}
            {c.href ? (
              <Link href={c.href} className="hover:text-fg cursor-pointer">
                {c.label}
              </Link>
            ) : (
              <span className="text-fg font-medium">{c.label}</span>
            )}
          </span>
        ))}
      </nav>
    </div>
  );
}
