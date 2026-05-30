"use client";

/**
 * AdvancedSection
 *
 * Collapsible "Advanced" section that lives above the modal footer. Hosts
 * the common-to-every-kind fields (`on_failure`, `always_run`,
 * `timeout_secs`, `working_dir`, `env_vars`, `capture`) so the per-kind
 * editor bodies can stay focused on the type-specific shape.
 *
 * `capture` is exposed as a single textarea showing the JSON form,
 * because the structured editor for capture rules would be a separate
 * feature in its own right.
 */

import { useMemo, useState } from "react";
import { Toggle } from "@/components/ui/Toggle";
import type { NewStep } from "../types";
import {
  FieldLabel,
  KeyValueEditor,
  MonoTextInput,
  NumberInput,
  Segmented,
  TextAreaInput,
} from "./primitives";

interface AdvancedSectionProps {
  step: NewStep;
  onChange: (next: NewStep) => void;
}

const ON_FAILURE_OPTIONS = [
  { value: "abort" as const, label: "abort" },
  { value: "continue" as const, label: "continue" },
];

export function AdvancedSection({ step, onChange }: AdvancedSectionProps) {
  const [open, setOpen] = useState(false);

  const envEntries = useMemo(
    () => Object.entries(step.env_vars ?? {}).map(([k, v]) => ({ key: k, value: v })),
    [step.env_vars],
  );

  const captureJson = useMemo(() => {
    try {
      return Object.keys(step.capture ?? {}).length > 0
        ? JSON.stringify(step.capture, null, 2)
        : "";
    } catch {
      return "";
    }
  }, [step.capture]);

  const onFailure: "abort" | "continue" =
    step.on_failure === "continue" ? "continue" : "abort";

  return (
    <div className="border-t border-border mt-2 pt-2">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="w-full flex items-center gap-2 px-1 py-2 text-eyebrow !text-fg-tertiary hover:!text-fg cursor-pointer"
        aria-expanded={open}
      >
        <span aria-hidden>{open ? "▾" : "▸"}</span>
        Advanced
        {!open && (
          <span className="ml-auto text-[10.5px] !text-fg-subtle normal-case tracking-normal font-sans">
            on_failure, always_run, timeout, working_dir, env_vars, capture…
          </span>
        )}
      </button>

      {open && (
        <div className="mt-2 space-y-4 pl-1">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <FieldLabel>On failure</FieldLabel>
              <Segmented
                value={onFailure}
                onChange={(next) => onChange({ ...step, on_failure: next } as NewStep)}
                options={ON_FAILURE_OPTIONS}
                ariaLabel="On failure"
              />
            </div>
            <label className="flex items-end justify-between gap-3 pb-1 cursor-pointer">
              <span className="text-sm text-fg-secondary">Always run</span>
              <Toggle
                checked={step.always_run ?? false}
                onChange={(v) => onChange({ ...step, always_run: v } as NewStep)}
                ariaLabel="Always run"
              />
            </label>
          </div>

          <div className="grid grid-cols-2 gap-3">
            <div>
              <FieldLabel>Timeout (seconds)</FieldLabel>
              <NumberInput
                value={step.timeout_secs ?? ""}
                onChange={(e) =>
                  onChange({
                    ...step,
                    timeout_secs: e.target.value.length > 0 ? parseInt(e.target.value, 10) : null,
                  } as NewStep)
                }
                placeholder="60"
              />
            </div>
            <div>
              <FieldLabel>Working directory</FieldLabel>
              <MonoTextInput
                value={step.working_dir ?? ""}
                onChange={(e) =>
                  onChange({
                    ...step,
                    working_dir: e.target.value.length > 0 ? e.target.value : null,
                  } as NewStep)
                }
                placeholder="/tmp"
              />
            </div>
          </div>

          <div>
            <FieldLabel>Environment variables</FieldLabel>
            <KeyValueEditor
              entries={envEntries}
              onChange={(next) => {
                const obj: Record<string, string> = {};
                for (const { key, value: v } of next) {
                  if (!key) continue;
                  obj[key] = v;
                }
                onChange({
                  ...step,
                  env_vars: Object.keys(obj).length > 0 ? obj : null,
                } as NewStep);
              }}
              keyPlaceholder="NAME"
              valuePlaceholder="value"
              addLabel="+ Add variable"
            />
          </div>

          <div>
            <FieldLabel>Capture (JSON)</FieldLabel>
            <TextAreaInput
              value={captureJson}
              onChange={(e) => {
                const raw = e.target.value;
                if (raw.trim().length === 0) {
                  onChange({ ...step, capture: {} } as NewStep);
                  return;
                }
                try {
                  const parsed = JSON.parse(raw) as Record<string, unknown>;
                  onChange({ ...step, capture: parsed } as NewStep);
                } catch {
                  // Swallow parse errors — the user can keep typing.
                }
              }}
              placeholder='{ "exports": { "user": "stdout" } }'
              spellCheck={false}
              className="min-h-[80px]"
            />
          </div>
        </div>
      )}
    </div>
  );
}
