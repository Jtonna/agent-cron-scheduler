"use client";

/**
 * SidebarSectionHeading
 *
 * Small uppercase heading used to title sidebar sections. The
 * `first:mt-0` margin trick collapses the top margin when this is the
 * very first item in the sidebar, so consecutive sections breathe
 * without an awkward gap above the first one.
 */

interface SidebarSectionHeadingProps {
  children: React.ReactNode;
  className?: string;
}

export function SidebarSectionHeading({ children, className = "" }: SidebarSectionHeadingProps) {
  return (
    <div
      className={`px-2 mt-5 first:mt-0 mb-1.5 text-[11px] font-semibold text-fg-subtle uppercase tracking-wider ${className}`}
    >
      {children}
    </div>
  );
}
