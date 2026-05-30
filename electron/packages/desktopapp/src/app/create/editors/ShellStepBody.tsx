"use client";

/**
 * ShellStepBody
 *
 * Body of the step-editor modal when the selected step is a shell step.
 * Exposes the shell `command` (multiline monospace textarea) and the
 * `pass_stdin` toggle, plus the template-chip insertion helpers.
 *
 * NOTE: We deliberately avoid pulling monaco/codemirror — a styled
 * <textarea> is good enough for the prototype's fidelity.
 */

import { Toggle } from "@/components/ui/Toggle";
import type { NewShellStep } from "../types";
import {
  FieldLabel,
  TemplateChips,
  TextAreaInput,
  useCursorInsert,
} from "./primitives";

interface ShellStepBodyProps {
  value: NewShellStep;
  onChange: (next: NewShellStep) => void;
}

export function ShellStepBody({ value, onChange }: ShellStepBodyProps) {
  const [attach, onSelect, insert] = useCursorInsert<HTMLTextAreaElement>(value.command, (next) =>
    onChange({ ...value, command: next }),
  );

  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Command</FieldLabel>
        <TextAreaInput
          ref={attach}
          value={value.command}
          onSelect={onSelect}
          onChange={(e) => onChange({ ...value, command: e.target.value })}
          placeholder="echo hello"
          className="min-h-[180px] bg-surface-tertiary"
          spellCheck={false}
        />
        <div className="text-[10.5px] text-fg-muted mt-1">
          Templates expand at runtime — click a chip to insert.
        </div>
        <TemplateChips onInsert={insert} />
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
