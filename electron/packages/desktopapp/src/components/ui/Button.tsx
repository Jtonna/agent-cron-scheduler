"use client";

import type { ReactNode } from "react";
import Link from "next/link";
import { Button as AriaButton } from "react-aria-components";

/**
 * Button
 *
 * The app's primary action button. Polymorphic: renders a Link, a React
 * Aria Button, or a static span depending on which prop is provided.
 *
 * Intents (primary / secondary / ghost), sizes (sm / md / lg), and
 * shapes (pill / rounded) compose into the final visual. Use `fullWidth`
 * for sidebar-style stretch buttons.
 */

type ButtonIntent = "primary" | "secondary" | "ghost";
type ButtonSize = "sm" | "md" | "lg";
type ButtonShape = "pill" | "rounded";

type BaseButtonProps = {
  children: ReactNode;
  icon?: ReactNode;
  intent?: ButtonIntent;
  size?: ButtonSize;
  shape?: ButtonShape;
  fullWidth?: boolean;
  isDisabled?: boolean;
  className?: string;
};

type ButtonProps = BaseButtonProps &
  (
    | { href: string; onPress?: never; type?: never }
    | { onPress: () => void; href?: never; type?: "button" | "submit" | "reset" }
    | { href?: never; onPress?: never; type?: never }
  );

const INTENT_CLASSES: Record<ButtonIntent, string> = {
  primary: "bg-brand hover:bg-brand-hover text-surface",
  secondary: "bg-surface-secondary hover:bg-surface-tertiary text-fg border border-border",
  ghost: "text-fg-secondary hover:text-fg hover:bg-surface-hover",
};

const SIZE_CLASSES: Record<ButtonSize, string> = {
  sm: "px-3 py-1.5 text-xs gap-1.5 font-semibold",
  md: "px-5 py-2.5 text-sm gap-2 font-semibold",
  lg: "px-6 py-3 text-base gap-2.5 font-semibold",
};

const SHAPE_CLASSES: Record<ButtonShape, string> = {
  pill: "rounded-pill",
  rounded: "rounded-input",
};

function buildClassName({
  intent,
  size,
  shape,
  fullWidth,
  isDisabled,
  className,
}: Required<Pick<BaseButtonProps, "intent" | "size" | "shape" | "fullWidth">> &
  Pick<BaseButtonProps, "isDisabled" | "className">) {
  const layout = fullWidth ? "flex w-full justify-center" : "inline-flex";

  const base =
    "items-center transition-colors outline-none focus-visible:ring-2 focus-visible:ring-brand-ring focus-visible:ring-offset-2 cursor-pointer";

  const disabledClasses = isDisabled ? "disabled:opacity-50 disabled:cursor-not-allowed" : "";

  return [
    layout,
    base,
    INTENT_CLASSES[intent],
    SIZE_CLASSES[size],
    SHAPE_CLASSES[shape],
    disabledClasses,
    className,
  ]
    .filter(Boolean)
    .join(" ");
}

export function Button(props: ButtonProps) {
  const {
    children,
    icon,
    intent = "primary",
    size = "md",
    shape = "pill",
    fullWidth = false,
    isDisabled = false,
    className = "",
  } = props;

  const cls = buildClassName({
    intent,
    size,
    shape,
    fullWidth,
    isDisabled,
    className,
  });

  const content = (
    <>
      {icon}
      {children}
    </>
  );

  if ("href" in props && props.href !== undefined) {
    return (
      <Link href={props.href} className={cls} aria-disabled={isDisabled || undefined}>
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
        className={cls}
      >
        {content}
      </AriaButton>
    );
  }

  return <span className={cls}>{content}</span>;
}
