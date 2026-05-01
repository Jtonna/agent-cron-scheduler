import { ReactNode } from "react";

/**
 * Sidebar
 *
 * Layout shell for left/right rails — a flex column with consistent
 * padding. Compose with SidebarSection + Button + feature-specific
 * content. The page is responsible for the sticky positioning + scroll
 * container; this component just lays out the rail's interior.
 */
interface SidebarProps {
  children: ReactNode;
  className?: string;
}

export function Sidebar({ children, className = "" }: SidebarProps) {
  return <aside className={`p-3 flex flex-col gap-1 ${className}`}>{children}</aside>;
}
