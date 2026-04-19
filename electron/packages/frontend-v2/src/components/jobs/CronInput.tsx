"use client";

import React, { useRef, useState, useEffect } from "react";
import {
  parseCronFields,
  fieldsToExpression,
  cronToHuman,
  validateCron,
} from "@/lib/cron";
import { cn } from "@/lib/utils";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

interface CronInputProps {
  value: string;
  onChange: (expression: string) => void;
  error?: string;
}

const FIELD_LABELS = ["Minute", "Hour", "Day of Month", "Month", "Weekday"];
const FIELD_HINTS = ["0-59", "0-23", "1-31", "1-12", "0-7"];

const PRESETS = [
  { expr: "*/5 * * * *", label: "Every 5 min" },
  { expr: "*/15 * * * *", label: "Every 15 min" },
  { expr: "*/30 * * * *", label: "Every 30 min" },
  { expr: "0 * * * *", label: "Hourly" },
  { expr: "0 0 * * *", label: "Daily" },
  { expr: "0 0 * * 0", label: "Weekly" },
  { expr: "0 0 1 * *", label: "Monthly" },
];

const FIELD_KEYS: (keyof ReturnType<typeof parseCronFields>)[] = [
  "minute",
  "hour",
  "dayOfMonth",
  "month",
  "dayOfWeek",
];

export function CronInput({ value, onChange, error }: CronInputProps) {
  const inputRefs = useRef<(HTMLInputElement | null)[]>([]);

  // Local editing state — decoupled from the expression so fields can be empty mid-edit
  const [localFields, setLocalFields] = useState<string[]>(() => {
    const f = parseCronFields(value);
    return FIELD_KEYS.map((k) => f[k]);
  });

  // Sync from parent when value changes externally (e.g., initial load, job edit)
  const lastPushed = useRef(value);
  useEffect(() => {
    if (value !== lastPushed.current) {
      const f = parseCronFields(value);
      setLocalFields(FIELD_KEYS.map((k) => f[k]));
      lastPushed.current = value;
    }
  }, [value]);

  const pushToParent = (fields: string[]) => {
    const expression = fieldsToExpression({
      minute: fields[0] || "*",
      hour: fields[1] || "*",
      dayOfMonth: fields[2] || "*",
      month: fields[3] || "*",
      dayOfWeek: fields[4] || "*",
    });
    lastPushed.current = expression;
    onChange(expression);
  };

  const handleChange = (index: number, rawVal: string) => {
    const val = rawVal.trim();
    const updated = [...localFields];
    updated[index] = val;
    setLocalFields(updated);
    // Push to parent immediately (empty fields become * in the expression)
    pushToParent(updated);
  };

  const handleBlur = (index: number) => {
    if (!localFields[index]) {
      const updated = [...localFields];
      updated[index] = "*";
      setLocalFields(updated);
      pushToParent(updated);
    }
  };

  const handleKeyDown = (
    index: number,
    e: React.KeyboardEvent<HTMLInputElement>
  ) => {
    if (e.key === "Tab" || e.key === "ArrowRight") {
      if (index < 4) {
        e.preventDefault();
        inputRefs.current[index + 1]?.focus();
        inputRefs.current[index + 1]?.select();
      }
    } else if (e.key === "ArrowLeft") {
      if (index > 0) {
        e.preventDefault();
        inputRefs.current[index - 1]?.focus();
        inputRefs.current[index - 1]?.select();
      }
    }
  };

  // Use the actual expression (with * for empties) for validation/description
  const validationError = error || validateCron(value);
  const isValid = !validationError;
  const humanDescription = cronToHuman(value);

  return (
    <div className="flex flex-col gap-4 w-full">
      {/* Five field inputs */}
      <div className="flex gap-2 w-full items-end justify-center">
        {localFields.map((fieldVal, i) => (
          <div key={i} className="flex-1 flex flex-col gap-1 items-center">
            <span className="text-xs text-muted-foreground text-center">
              {FIELD_LABELS[i]}
            </span>
            <Input
              ref={(el) => {
                inputRefs.current[i] = el;
              }}
              type="text"
              value={fieldVal}
              onChange={(e) => handleChange(i, e.target.value)}
              onFocus={(e) => e.target.select()}
              onBlur={() => handleBlur(i)}
              onKeyDown={(e) => handleKeyDown(i, e)}
              placeholder="*"
              aria-label={FIELD_LABELS[i]}
              className={cn(
                "text-center font-mono text-lg font-semibold px-2 py-3",
                !isValid && "border-destructive focus-visible:ring-destructive"
              )}
            />
            <span className="text-xs text-muted-foreground text-center">
              {FIELD_HINTS[i]}
            </span>
          </div>
        ))}
      </div>

      {/* Human-readable description panel */}
      <div
        className={cn(
          "flex items-center gap-2 rounded-md px-4 py-3 w-full",
          isValid
            ? "bg-emerald-500/10 border border-emerald-500/30"
            : "bg-destructive/10 border border-destructive/30"
        )}
      >
        <span
          className={cn(
            "text-sm",
            isValid
              ? "text-emerald-600 dark:text-emerald-400"
              : "text-destructive"
          )}
        >
          {humanDescription}
        </span>
      </div>

      {/* Quick Apply presets */}
      <div className="flex flex-col gap-2 w-full">
        <span className="text-xs text-muted-foreground">Quick Apply</span>
        <div className="flex flex-wrap gap-2">
          {PRESETS.map((preset) => (
            <Button
              key={preset.expr}
              type="button"
              variant="outline"
              size="sm"
              className="font-mono text-xs"
              onClick={() => {
                const f = parseCronFields(preset.expr);
                const updated = FIELD_KEYS.map((k) => f[k]);
                setLocalFields(updated);
                pushToParent(updated);
              }}
            >
              {preset.label}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}
