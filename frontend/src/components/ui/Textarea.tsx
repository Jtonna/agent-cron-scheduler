"use client";

import React from "react";
import { Textarea as OnceTextarea } from "@once-ui-system/core";

interface TextareaProps
  extends Omit<React.TextareaHTMLAttributes<HTMLTextAreaElement>, 'size'> {
  label?: string;
  error?: string;
}

export function Textarea({
  label,
  error,
  className = "",
  id,
  ...props
}: TextareaProps) {
  const textareaId = id || label?.toLowerCase().replace(/\s+/g, "-");

  return (
    <OnceTextarea
      id={textareaId || ""}
      label={label}
      error={error ? true : false}
      errorMessage={error}
      className={className}
      lines={4}
      {...(props as Record<string, unknown>)}
    />
  );
}
