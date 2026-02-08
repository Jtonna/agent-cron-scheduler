"use client";

import React from "react";
import { Input as OnceInput } from "@once-ui-system/core";

interface InputProps extends Omit<React.InputHTMLAttributes<HTMLInputElement>, 'size'> {
  label?: string;
  error?: string;
  helper?: string;
}

export function Input({
  label,
  error,
  helper,
  className = "",
  id,
  ...props
}: InputProps) {
  const inputId = id || label?.toLowerCase().replace(/\s+/g, "-");

  return (
    <OnceInput
      id={inputId || ""}
      label={label}
      error={error ? true : false}
      errorMessage={error}
      description={helper && !error ? helper : undefined}
      className={className}
      {...(props as Record<string, unknown>)}
    />
  );
}
