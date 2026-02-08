"use client";

import React from "react";
import { Button as OnceButton } from "@once-ui-system/core";

type ButtonVariant = "primary" | "secondary" | "danger" | "ghost";
type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant;
  size?: ButtonSize;
  children: React.ReactNode;
}

const variantMap: Record<ButtonVariant, "primary" | "secondary" | "tertiary" | "danger"> = {
  primary: "primary",
  secondary: "secondary",
  danger: "danger",
  ghost: "tertiary",
};

const sizeMap: Record<ButtonSize, "s" | "m" | "l"> = {
  sm: "s",
  md: "m",
  lg: "l",
};

export function Button({
  variant = "primary",
  size = "md",
  className = "",
  children,
  disabled,
  type,
  ...props
}: ButtonProps) {
  return (
    <OnceButton
      variant={variantMap[variant]}
      size={sizeMap[size]}
      className={className}
      disabled={disabled}
      type={type}
      {...(props as Record<string, unknown>)}
    >
      {children}
    </OnceButton>
  );
}
