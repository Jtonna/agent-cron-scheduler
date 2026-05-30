"use client";

/**
 * CanvasBreadcrumb
 *
 * Tiny floating breadcrumb chip rendered between the Navbar and the
 * editor canvas. Stand-in for hosting a breadcrumb inside Navbar
 * itself (which currently doesn't accept children). When the Navbar
 * grows a `children` slot, this can be inlined there.
 *
 * Crumbs are one of two shapes via a discriminated union:
 *   - `BreadcrumbLinkItem` — static label, optionally a link.
 *   - `BreadcrumbEditableItem` — click-to-edit text input. Used to host
 *     the workflow name in the editor so the name doesn't have to live
 *     on the whiteboard. Looks like the surrounding text until focused
 *     (focus reveals the input chrome + ring).
 */

import Link from "next/link";
import { ChevronRight } from "lucide-react";
import { useEffect, useRef, useState } from "react";

export interface BreadcrumbLinkItem {
  kind?: "link";
  label: string;
  href?: string;
}

export interface BreadcrumbEditableItem {
  kind: "editable";
  value: string;
  onChange: (next: string) => void;
  placeholder?: string;
  ariaLabel?: string;
}

export type BreadcrumbItem = BreadcrumbLinkItem | BreadcrumbEditableItem;

interface CanvasBreadcrumbProps {
  crumbs: BreadcrumbItem[];
}

export function CanvasBreadcrumb({ crumbs }: CanvasBreadcrumbProps) {
  return (
    <div className="border-b border-border-subtle bg-surface px-8 py-1.5">
      <nav aria-label="Breadcrumb" className="flex items-center gap-1 text-[11.5px] text-fg-muted">
        {crumbs.map((c, i) => (
          <span key={i} className="inline-flex items-center gap-1">
            {i > 0 && <ChevronRight size={11} className="text-fg-subtle" aria-hidden />}
            {renderCrumb(c)}
          </span>
        ))}
      </nav>
    </div>
  );
}

function renderCrumb(crumb: BreadcrumbItem) {
  if (crumb.kind === "editable") {
    return <EditableCrumb crumb={crumb} />;
  }
  if (crumb.href) {
    return (
      <Link
        href={crumb.href}
        className="hover:text-fg cursor-pointer rounded-input px-0.5 focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
      >
        {crumb.label}
      </Link>
    );
  }
  return <span className="text-fg font-medium">{crumb.label}</span>;
}

function EditableCrumb({ crumb }: { crumb: BreadcrumbEditableItem }) {
  const [editing, setEditing] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const placeholder = crumb.placeholder ?? "Untitled";

  useEffect(() => {
    if (editing) inputRef.current?.focus();
  }, [editing]);

  if (editing) {
    return (
      <input
        ref={inputRef}
        type="text"
        value={crumb.value}
        onChange={(e) => crumb.onChange(e.target.value)}
        onBlur={() => setEditing(false)}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === "Escape") {
            e.preventDefault();
            setEditing(false);
          }
        }}
        placeholder={placeholder}
        aria-label={crumb.ariaLabel ?? "Edit name"}
        className="text-[11.5px] text-fg font-medium bg-surface-hover border border-border rounded-input px-1.5 py-0.5 outline-none focus:ring-2 focus:ring-brand-ring/40 focus:border-fg min-w-[180px]"
      />
    );
  }

  return (
    <button
      type="button"
      onClick={() => setEditing(true)}
      aria-label={crumb.ariaLabel ?? "Edit name"}
      className="text-fg font-medium hover:bg-surface-hover rounded-input px-1.5 py-0.5 cursor-text text-left max-w-[420px] truncate focus:outline-none focus-visible:ring-2 focus-visible:ring-brand-ring/40"
    >
      {crumb.value || <span className="text-fg-subtle font-normal">{placeholder}</span>}
    </button>
  );
}
