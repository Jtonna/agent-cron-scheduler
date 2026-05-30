"use client";

/**
 * ScriptStepBody
 *
 * Path-based script step editor. The backend only accepts a `path` +
 * `script_type` (no inline source body), so this form is intentionally
 * compact: type segmented control, path input, args, pass-stdin toggle.
 *
 * TODO: a future file-exists health check could light up an indicator
 * here. Browser sandbox makes this unverifiable from the desktop app
 * directly — would need a backend endpoint.
 */

import { Toggle } from "@/components/ui/Toggle";
import type { NewScriptStep, ScriptType } from "../types";
import {
  FieldLabel,
  MonoTextInput,
  Segmented,
  TemplateChips,
  useCursorInsert,
} from "./primitives";

interface ScriptStepBodyProps {
  value: NewScriptStep;
  onChange: (next: NewScriptStep) => void;
}

const SCRIPT_TYPE_OPTIONS: { value: ScriptType; label: string }[] = [
  { value: "shell", label: "shell" },
  { value: "batch", label: "batch" },
  { value: "python", label: "python" },
  { value: "powershell", label: "powershell" },
];

export function ScriptStepBody({ value, onChange }: ScriptStepBodyProps) {
  const args = (value.args ?? []).join(" ");
  const [attachArgs, onArgsSelect, insertIntoArgs] = useCursorInsert<HTMLInputElement>(
    args,
    (next) => onChange({ ...value, args: next.split(/\s+/).filter(Boolean) }),
  );

  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Script type</FieldLabel>
        <Segmented
          value={value.script_type}
          onChange={(next) => onChange({ ...value, script_type: next })}
          options={SCRIPT_TYPE_OPTIONS}
          ariaLabel="Script type"
        />
      </div>

      <div>
        <FieldLabel>Script path</FieldLabel>
        <MonoTextInput
          value={value.path}
          onChange={(e) => onChange({ ...value, path: e.target.value })}
          placeholder="./scripts/run.sh"
        />
      </div>

      <div>
        <FieldLabel>Arguments</FieldLabel>
        <MonoTextInput
          ref={attachArgs}
          value={args}
          onSelect={onArgsSelect}
          onChange={(e) =>
            onChange({ ...value, args: e.target.value.split(/\s+/).filter(Boolean) })
          }
          placeholder="--verbose --format json"
        />
        <div className="text-[10.5px] text-fg-muted mt-1">
          Whitespace-split. Quotes for spaces aren&apos;t supported in this prototype.
        </div>
        <TemplateChips onInsert={insertIntoArgs} />
      </div>

      <label className="flex items-center justify-between gap-3 py-1 cursor-pointer">
        <span className="text-sm text-fg-secondary">Pipe stdin from previous step</span>
        <Toggle
          checked={value.pass_stdin ?? false}
          onChange={(v) => onChange({ ...value, pass_stdin: v })}
          ariaLabel="Pipe stdin"
        />
      </label>
    </div>
  );
}
