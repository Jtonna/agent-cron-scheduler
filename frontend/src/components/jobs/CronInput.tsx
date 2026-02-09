"use client";

import React, { useRef, useState, useEffect } from "react";
import { Column, Row, Text } from "@once-ui-system/core";
import { parseCronFields, fieldsToExpression, cronToHuman, validateCron } from "@/lib/cron";

interface CronInputProps {
  value: string;
  onChange: (expression: string) => void;
  error?: string;
}

const FIELD_LABELS = ["Minute", "Hour", "Day", "Month", "Weekday"];
const FIELD_HINTS = ["0-59", "0-23", "1-31", "1-12", "0-6"];

const EXAMPLES = [
  { expr: "*/5 * * * *", desc: "Every 5 minutes" },
  { expr: "0 9 * * 1-5", desc: "Weekdays at 9:00 AM" },
  { expr: "30 */2 * * *", desc: "Every 2 hours at :30" },
  { expr: "0 0 1 * *", desc: "1st of every month at midnight" },
  { expr: "0 8 * * 1,3,5", desc: "Mon, Wed, Fri at 8:00 AM" },
  { expr: "0 */6 * * *", desc: "Every 6 hours" },
  { expr: "0 22 * * 0", desc: "Sundays at 10:00 PM" },
];
const FIELD_KEYS: (keyof ReturnType<typeof parseCronFields>)[] = [
  "minute", "hour", "dayOfMonth", "month", "dayOfWeek",
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

  const handleKeyDown = (index: number, e: React.KeyboardEvent<HTMLInputElement>) => {
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
    <Column gap="16" fillWidth>
      {/* Field boxes */}
      <Row gap="8" fillWidth horizontal="center" vertical="end">
        {localFields.map((fieldVal, i) => (
          <Column key={i} gap="4" horizontal="center" style={{ flex: 1 }}>
            <Text
              variant="label-default-xs"
              onBackground="neutral-weak"
              style={{ textAlign: "center" }}
            >
              {FIELD_LABELS[i]}
            </Text>
            <input
              ref={(el) => { inputRefs.current[i] = el; }}
              type="text"
              value={fieldVal}
              onChange={(e) => handleChange(i, e.target.value)}
              onFocus={(e) => e.target.select()}
              onBlur={() => handleBlur(i)}
              onKeyDown={(e) => handleKeyDown(i, e)}
              placeholder="*"
              style={{
                width: "100%",
                textAlign: "center",
                fontFamily: "var(--font-code)",
                fontSize: "var(--font-size-heading-s)",
                fontWeight: 600,
                padding: "12px 8px",
                border: `1px solid var(${isValid ? "--neutral-border-medium" : "--danger-border-medium"})`,
                borderRadius: "var(--radius-m)",
                background: "var(--surface)",
                color: "var(--neutral-on-background-strong)",
                outline: "none",
              }}
            />
            <Text
              variant="body-default-xs"
              onBackground="neutral-weak"
              style={{ textAlign: "center" }}
            >
              {FIELD_HINTS[i]}
            </Text>
          </Column>
        ))}
      </Row>

      {/* Human-readable description */}
      <Row
        paddingX="16" paddingY="12" radius="m" gap="8"
        fillWidth vertical="center"
        style={{
          background: isValid
            ? "var(--success-alpha-weak)"
            : "var(--danger-alpha-weak)",
          border: `1px solid var(${isValid ? "--success-border-medium" : "--danger-border-medium"})`,
        }}
      >
        <Text
          variant="body-default-s"
          style={{
            color: isValid
              ? "var(--success-on-background-strong)"
              : "var(--danger-on-background-strong)",
          }}
        >
          {humanDescription}
        </Text>
      </Row>

      {/* Quick examples */}
      <Column gap="8" fillWidth>
        <Text variant="label-default-xs" onBackground="neutral-weak">
          Quick Apply
        </Text>
        <Row gap="8" wrap>
          {EXAMPLES.map((ex) => (
            <button
              key={ex.expr}
              type="button"
              onClick={() => {
                const f = parseCronFields(ex.expr);
                const updated = FIELD_KEYS.map((k) => f[k]);
                setLocalFields(updated);
                pushToParent(updated);
              }}
              style={{
                padding: "4px 10px",
                borderRadius: "var(--radius-s)",
                border: "1px solid var(--neutral-border-medium)",
                background: "var(--surface)",
                color: "var(--neutral-on-background-medium)",
                fontFamily: "var(--font-code)",
                fontSize: "var(--font-size-body-xs)",
                cursor: "pointer",
              }}
            >
              <span style={{ color: "var(--neutral-on-background-strong)" }}>
                {ex.expr}
              </span>
              {" — "}
              {ex.desc}
            </button>
          ))}
        </Row>
      </Column>
    </Column>
  );
}
