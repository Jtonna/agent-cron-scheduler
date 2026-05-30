"use client";

/**
 * SetVarStepBody
 *
 * Editor body for a `set_var` step. Renders the `exports` map as a
 * key/value editor table.
 */

import { useMemo } from "react";
import type { NewSetVarStep } from "../types";
import { FieldLabel, KeyValueEditor, TemplateChips } from "./primitives";

interface SetVarStepBodyProps {
  value: NewSetVarStep;
  onChange: (next: NewSetVarStep) => void;
}

export function SetVarStepBody({ value, onChange }: SetVarStepBodyProps) {
  const entries = useMemo(
    () => Object.entries(value.exports ?? {}).map(([k, v]) => ({ key: k, value: v })),
    [value.exports],
  );

  return (
    <div className="space-y-3">
      <FieldLabel>Exports</FieldLabel>
      <KeyValueEditor
        entries={entries}
        onChange={(next) => {
          const obj: Record<string, string> = {};
          for (const { key, value: v } of next) {
            if (!key) continue;
            obj[key] = v;
          }
          onChange({ ...value, exports: obj });
        }}
        keyPlaceholder="VAR_NAME"
        valuePlaceholder="${input.something}"
        addLabel="+ Add export"
      />
      <div className="text-[10.5px] text-fg-muted">
        Template chips you can paste into values:
      </div>
      <TemplateChips onInsert={() => { /* no field focus to insert into — informational only */ }} />
    </div>
  );
}
