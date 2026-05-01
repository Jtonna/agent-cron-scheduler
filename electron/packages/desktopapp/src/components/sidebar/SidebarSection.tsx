"use client";

import { Children } from "react";
import { SidebarSectionHeading } from "./SidebarSectionHeading";

/**
 * SidebarSection
 *
 * Reusable sidebar section shell — heading + content slot. Pass children
 * for the populated state; provide `emptyText` to render a graceful empty
 * state when no children are supplied.
 */

interface SidebarSectionProps {
  title: string;
  children?: React.ReactNode;
  /**
   * If provided AND no children are rendered, the section displays this
   * text as a faint italic empty state under the heading.
   */
  emptyText?: string;
}

export function SidebarSection({ title, children, emptyText }: SidebarSectionProps) {
  const hasChildren = Children.count(children) > 0;
  return (
    <>
      <SidebarSectionHeading>{title}</SidebarSectionHeading>
      {hasChildren ? (
        children
      ) : emptyText ? (
        <div className="px-2 py-2 text-xs text-fg-subtle italic">{emptyText}</div>
      ) : null}
    </>
  );
}
