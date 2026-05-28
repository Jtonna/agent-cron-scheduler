import { ReactNode } from "react";

/**
 * StatWidget
 *
 * Generic widget shell — used by Cost, Health, TopSpenders, and the
 * cost-trend widget. Renders a card with a title row + body slot.
 *
 * Visual language uplift (acs-ui-refresh): the title row uses the
 * mono-uppercase "eyebrow" treatment so widget headers share the same
 * micro-label language as the hero carousel. Borders and surfaces are
 * unchanged so widgets still read as quiet data cards alongside the
 * louder pastel persona slides.
 */

interface StatWidgetProps {
  title: string;
  icon?: ReactNode;
  children: ReactNode;
  className?: string;
}

export function StatWidget({ title, icon, children, className = "" }: StatWidgetProps) {
  return (
    <div className={`bg-surface border border-border rounded-card p-5 ${className}`}>
      <div className="text-eyebrow flex items-center gap-2 mb-4 !text-fg-tertiary">
        {icon && <span className="text-fg-subtle shrink-0">{icon}</span>}
        <span>{title}</span>
      </div>
      <div>{children}</div>
    </div>
  );
}
