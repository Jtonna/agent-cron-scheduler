"use client"

import * as React from "react"
import { cn } from "@/lib/utils"

interface PillToggleProps {
  options: { label: string; value: string }[]
  value: string
  onChange: (value: string) => void
  className?: string
}

const PillToggle = React.forwardRef<HTMLDivElement, PillToggleProps>(
  ({ options, value, onChange, className }, ref) => {
    return (
      <div
        ref={ref}
        className={cn("inline-flex items-center bg-muted rounded-full p-1", className)}
      >
        {options.map((option) => (
          <button
            key={option.value}
            onClick={() => onChange(option.value)}
            className={cn(
              "px-5 py-1.5 rounded-full text-sm font-medium transition-all duration-200 cursor-pointer",
              value === option.value
                ? "bg-primary text-primary-foreground"
                : "text-muted-foreground hover:text-foreground"
            )}
          >
            {option.label}
          </button>
        ))}
      </div>
    )
  }
)
PillToggle.displayName = "PillToggle"

export { PillToggle }
