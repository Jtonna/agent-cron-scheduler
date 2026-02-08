"use client";

import React from "react";
import { Switch } from "@once-ui-system/core";

interface ToggleProps {
  checked: boolean;
  onChange: (checked: boolean) => void;
  label?: string;
  disabled?: boolean;
}

export function Toggle({ checked, onChange, label, disabled }: ToggleProps) {
  return (
    <Switch
      id={label?.toLowerCase().replace(/\s+/g, "-") || "toggle"}
      label={label}
      isChecked={checked}
      onToggle={() => onChange(!checked)}
      disabled={disabled}
    />
  );
}
