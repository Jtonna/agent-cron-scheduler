"use client";

import type { ReactNode } from "react";
import Link from "next/link";
import { ToggleButton } from "react-aria-components";

/**
 * Pill
 *
 * Polymorphic pill primitive. Render mode is determined by which prop is
 * provided:
 *  - `href`     → renders a `next/link` Link (used by FavoritedJobs, FilterTabs)
 *  - `onChange` → renders a React Aria ToggleButton (selectable form)
 *  - neither    → renders a static `<span>` (decorative)
 *
 * Sizing affects only inner padding/typography. The pill itself is always
 * `rounded-pill` and `whitespace-nowrap`.
 */

type PillSize = "sm" | "md";

type BasePillProps = {
  label: string;
  icon?: ReactNode;
  active?: boolean;
  bordered?: boolean;
  size?: PillSize;
  className?: string;
};

type PillProps = BasePillProps &
  (
    | { href: string; onChange?: never }
    | { onChange: (selected: boolean) => void; href?: never }
    | { href?: never; onChange?: never }
  );

const SIZE_CLASSES: Record<PillSize, string> = {
  sm: "px-2.5 py-1 text-xs gap-1.5 font-medium",
  md: "px-4 py-2 text-sm gap-2 font-semibold",
};

function buildClassName({
  active = false,
  bordered = false,
  size = "sm",
  className = "",
}: Required<Pick<BasePillProps, "size">> &
  Pick<BasePillProps, "active" | "bordered" | "className">) {
  const base =
    "inline-flex items-center rounded-pill whitespace-nowrap transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring";

  const sizing = SIZE_CLASSES[size];

  let stateClasses: string;
  if (active) {
    stateClasses = bordered
      ? "bg-fg text-surface border border-border-active"
      : "bg-fg text-surface";
  } else {
    stateClasses = bordered
      ? "text-fg-secondary hover:text-fg hover:bg-surface-hover border border-border hover:border-border-strong"
      : "text-fg-secondary hover:text-fg hover:bg-surface-hover";
  }

  return [base, sizing, stateClasses, className].filter(Boolean).join(" ");
}

export function Pill(props: PillProps) {
  const { label, icon, active = false, bordered = false, size = "sm", className = "" } = props;

  const cls = buildClassName({ active, bordered, size, className });

  const content = (
    <>
      {icon}
      {label}
    </>
  );

  if ("href" in props && props.href !== undefined) {
    return (
      <Link href={props.href} className={cls}>
        {content}
      </Link>
    );
  }

  if ("onChange" in props && props.onChange !== undefined) {
    return (
      <ToggleButton
        isSelected={active}
        onChange={props.onChange}
        className={`${cls} cursor-pointer`}
      >
        {content}
      </ToggleButton>
    );
  }

  return <span className={cls}>{content}</span>;
}
