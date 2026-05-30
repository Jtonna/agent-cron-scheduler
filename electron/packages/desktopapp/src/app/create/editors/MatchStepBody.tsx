"use client";

/**
 * MatchStepBody
 *
 * Editor body for a `match` step. Shows the expression input, a list of
 * case rows (each with an editable key and a count badge), and a
 * "Default branch" row at the bottom. Clicking a case (or the default
 * row) calls `onDrillIn(caseKey | null)` so the parent can open a
 * nested modal with the appropriate breadcrumb crumb.
 */

import { ChevronRight, X } from "lucide-react";
import type { NewMatchStep } from "../types";
import {
  FieldLabel,
  MonoTextInput,
  TemplateChips,
  useCursorInsert,
} from "./primitives";

interface MatchStepBodyProps {
  value: NewMatchStep;
  onChange: (next: NewMatchStep) => void;
  /** Called when the user wants to drill into a case (or null for default). */
  onDrillIn: (caseKey: string | null) => void;
}

export function MatchStepBody({ value, onChange, onDrillIn }: MatchStepBodyProps) {
  const [attachExpr, onExprSelect, insertIntoExpr] = useCursorInsert<HTMLInputElement>(
    value.expr,
    (next) => onChange({ ...value, expr: next }),
  );

  const cases = Object.entries(value.cases ?? {});

  const renameCase = (oldKey: string, newKey: string) => {
    if (oldKey === newKey) return;
    if (!newKey.trim() || value.cases[newKey]) return;
    const nextCases: Record<string, typeof value.cases[string]> = {};
    for (const [k, steps] of Object.entries(value.cases)) {
      nextCases[k === oldKey ? newKey : k] = steps;
    }
    onChange({ ...value, cases: nextCases });
  };

  const deleteCase = (key: string) => {
    const next = { ...value.cases };
    delete next[key];
    onChange({ ...value, cases: next });
  };

  const addCase = () => {
    let suffix = cases.length + 1;
    let name = `case_${suffix}`;
    while (value.cases[name]) {
      suffix += 1;
      name = `case_${suffix}`;
    }
    onChange({ ...value, cases: { ...value.cases, [name]: [] } });
  };

  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Match expression</FieldLabel>
        <MonoTextInput
          ref={attachExpr}
          value={value.expr}
          onSelect={onExprSelect}
          onChange={(e) => onChange({ ...value, expr: e.target.value })}
          placeholder="${steps.previous.exports.status}"
        />
        <TemplateChips onInsert={insertIntoExpr} />
      </div>

      <div>
        <FieldLabel>Cases</FieldLabel>
        <div className="space-y-1.5 max-h-[260px] overflow-y-auto pr-1">
          {cases.map(([key, steps]) => (
            <div
              key={key}
              className="grid grid-cols-[1fr_auto_auto_auto] items-center gap-2 bg-surface border border-border rounded-input px-2 py-1.5"
            >
              <MonoTextInput
                value={key}
                onChange={(e) => renameCase(key, e.target.value)}
                aria-label="Case key"
                className="!py-1 !text-[12px]"
              />
              <span className="text-[11px] text-fg-muted bg-surface-tertiary px-2 py-0.5 rounded-pill whitespace-nowrap">
                {steps.length} step{steps.length === 1 ? "" : "s"}
              </span>
              <button
                type="button"
                onClick={() => onDrillIn(key)}
                className="text-fg-muted hover:text-fg p-1 rounded-input cursor-pointer flex items-center"
                aria-label={`Drill into case ${key}`}
                title="Edit case steps"
              >
                <ChevronRight size={14} />
              </button>
              <button
                type="button"
                onClick={() => deleteCase(key)}
                className="text-fg-subtle hover:text-status-failed p-1 rounded-input cursor-pointer flex items-center"
                aria-label={`Delete case ${key}`}
              >
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
        <button
          type="button"
          onClick={addCase}
          className="mt-1.5 w-full text-left bg-transparent border border-dashed border-border text-fg-muted hover:text-fg hover:border-fg text-[11px] px-2.5 py-1.5 rounded-input cursor-pointer transition-colors"
        >
          + Add case
        </button>
      </div>

      <div>
        <FieldLabel>Default branch</FieldLabel>
        <div className="grid grid-cols-[1fr_auto_auto] items-center gap-2 bg-surface-secondary border border-dashed border-border rounded-input px-2.5 py-1.5">
          <span className="text-sm text-fg-muted italic">default</span>
          <span className="text-[11px] text-fg-muted bg-surface-tertiary px-2 py-0.5 rounded-pill whitespace-nowrap">
            {(value.default ?? []).length} step{(value.default ?? []).length === 1 ? "" : "s"}
          </span>
          <button
            type="button"
            onClick={() => onDrillIn(null)}
            className="text-fg-muted hover:text-fg p-1 rounded-input cursor-pointer flex items-center"
            aria-label="Drill into default branch"
            title="Edit default branch"
          >
            <ChevronRight size={14} />
          </button>
        </div>
      </div>
    </div>
  );
}
