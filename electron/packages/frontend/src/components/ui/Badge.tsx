"use client";

import React from "react";
import { Tag } from "@once-ui-system/core";

type BadgeVariant = "success" | "error" | "running" | "warning" | "disabled" | "default";

interface BadgeProps {
  variant?: BadgeVariant;
  children: React.ReactNode;
  className?: string;
}

const variantMap: Record<BadgeVariant, "success" | "danger" | "warning" | "info" | "neutral" | "brand"> = {
  success: "success",
  error: "danger",
  running: "info",
  warning: "warning",
  disabled: "neutral",
  default: "neutral",
};

export function Badge({
  variant = "default",
  children,
}: BadgeProps) {
  return (
    <Tag
      variant={variantMap[variant]}
      size="s"
      label={typeof children === "string" ? children : undefined}
    >
      {typeof children !== "string" ? children : undefined}
    </Tag>
  );
}

export function statusToBadgeVariant(
  status: string,
  enabled?: boolean
): BadgeVariant {
  if (enabled === false) return "disabled";
  switch (status) {
    case "Completed":
      return "success";
    case "Failed":
      return "error";
    case "Running":
      return "running";
    case "Killed":
      return "warning";
    default:
      return "default";
  }
}
