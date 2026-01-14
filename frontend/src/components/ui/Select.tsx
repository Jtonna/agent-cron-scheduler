"use client";

import React from "react";
import { Select as OnceSelect } from "@once-ui-system/core";

export interface SelectOption {
  label: string;
  value: string;
}

interface SelectProps {
  label?: string;
  options: SelectOption[];
  value: string;
  onSelect: (value: string) => void;
  id?: string;
  placeholder?: string;
  searchable?: boolean;
}

export function Select({
  label,
  options,
  value,
  onSelect,
  id,
  placeholder,
  searchable,
}: SelectProps) {
  const selectId = id || label?.toLowerCase().replace(/\s+/g, "-") || "select";

  return (
    <OnceSelect
      id={selectId}
      label={label || undefined}
      value={value}
      options={options}
      onSelect={onSelect}
      fillWidth
      placeholder={placeholder}
      searchable={searchable}
    />
  );
}
