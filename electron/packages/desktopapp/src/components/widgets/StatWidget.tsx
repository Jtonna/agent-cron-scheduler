import { ReactNode } from "react";

/**
 * StatWidget
 *
 * Generic widget shell — used by Cost, Health, TopSpenders, and the
 * cost-trend widget. Renders a card with a title row + body slot.
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
      <div className="flex items-center gap-2 mb-4">
        {icon && <span className="text-fg-subtle shrink-0">{icon}</span>}
        <span className="text-xs font-semibold text-fg-subtle uppercase tracking-wider">
          {title}
        </span>
      </div>
      <div>{children}</div>
    </div>
  );
}
