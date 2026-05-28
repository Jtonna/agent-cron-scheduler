"use client";

import type { ReactNode } from "react";
import Link from "next/link";
import { Button as AriaButton } from "react-aria-components";

/**
 * CompactActionButton
 *
 * The small (xs / sidebar-density) action button used across all three
 * sidebars — "Run Workflow", "Edit Workflow", "Delete", "New Workflow",
 * the chevron half of the Run split button, and so on. Replaces three
 * separate inline className stacks that had drifted out of alignment.
 *
 * Visually a fixed compact density: `px-3 py-1.5 text-xs font-semibold
 * rounded-input`. Intent picks the colour treatment. Polymorphic: pass
 * `href` (Next.js Link), `onPress` (React Aria Button), or neither
 * (static span). The shape and density are deliberately not configurable
 * — the whole point of this component is that every sidebar action
 * matches every other one.
 *
 * Lives in `ui/` because it is domain-agnostic: any compact action
 * surface (the sidebars today, future toolbar action rows tomorrow) can
 * reach for it. Sibling of `Button` rather than a `Button` variant
 * because `Button`'s sm density currently enforces a pill shape and
 * adding a `shape="rect"` would invalidate existing call-sites that
 * lean on the pill-by-default contract.
 */

type CompactIntent = "primary" | "secondary" | "ghost" | "destructive";

type BaseProps = {
  children: ReactNode;
  icon?: ReactNode;
  intent?: CompactIntent;
  fullWidth?: boolean;
  isDisabled?: boolean;
  ariaLabel?: string;
  className?: string;
};

type CompactActionButtonProps = BaseProps &
  (
    | { href: string; onPress?: never; type?: never }
    | { onPress: () => void; href?: never; type?: "button" | "submit" | "reset" }
    | { href?: never; onPress?: never; type?: never }
  );

const INTENT_CLASSES: Record<CompactIntent, string> = {
  primary: "bg-brand hover:bg-brand-hover text-surface",
  secondary:
    "bg-surface-secondary hover:bg-surface-tertiary text-fg border border-border",
  ghost: "text-fg-secondary hover:text-fg hover:bg-surface-hover",
  destructive: "text-status-failed hover:bg-status-failed-bg",
};

const BASE =
  "inline-flex items-center justify-center gap-1.5 px-3 py-1.5 text-xs font-semibold rounded-input outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2 cursor-pointer transition-colors disabled:opacity-50 disabled:cursor-not-allowed";

function buildClassName(
  intent: CompactIntent,
  fullWidth: boolean,
  className: string,
) {
  const layout = fullWidth ? "flex w-full" : "";
  return [layout, BASE, INTENT_CLASSES[intent], className]
    .filter(Boolean)
    .join(" ");
}

export function CompactActionButton(props: CompactActionButtonProps) {
  const {
    children,
    icon,
    intent = "secondary",
    fullWidth = false,
    isDisabled = false,
    ariaLabel,
    className = "",
  } = props;

  const cls = buildClassName(intent, fullWidth, className);
  const content = (
    <>
      {icon}
      {children}
    </>
  );

  if ("href" in props && props.href !== undefined) {
    return (
      <Link
        href={props.href}
        className={cls}
        aria-label={ariaLabel}
        aria-disabled={isDisabled || undefined}
      >
        {content}
      </Link>
    );
  }

  if ("onPress" in props && props.onPress !== undefined) {
    return (
      <AriaButton
        type={props.type ?? "button"}
        onPress={props.onPress}
        isDisabled={isDisabled}
        aria-label={ariaLabel}
        className={cls}
      >
        {content}
      </AriaButton>
    );
  }

  return (
    <span className={cls} aria-label={ariaLabel}>
      {content}
    </span>
  );
}
