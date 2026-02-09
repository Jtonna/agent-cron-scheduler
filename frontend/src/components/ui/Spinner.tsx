"use client";

import React from "react";
import { Spinner as OnceSpinner } from "@once-ui-system/core";

interface SpinnerProps {
  size?: "sm" | "md" | "lg";
  className?: string;
}

const sizeMap = {
  sm: "s",
  md: "m",
  lg: "l",
} as const;

export function Spinner({ size = "md", className = "" }: SpinnerProps) {
  return (
    <OnceSpinner
      size={sizeMap[size]}
      className={className}
    />
  );
}
