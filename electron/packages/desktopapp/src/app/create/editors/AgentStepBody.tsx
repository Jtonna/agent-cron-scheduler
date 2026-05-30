"use client";

/**
 * AgentStepBody
 *
 * Editor body for an `agent` step. Currently only supports the
 * `claude_code_cli` agent kind — the select is left in place for the
 * eventual addition of more backends. Includes a `model` input, an
 * extra-args chip input, the prose-style prompt textarea, and a coarse
 * "~N tokens" estimate computed from prompt length / 4 (decorative).
 */

import type { NewAgentStep } from "../types";
import {
  ChipInput,
  FieldLabel,
  MonoTextInput,
  TemplateChips,
  TextAreaInput,
  useCursorInsert,
} from "./primitives";

interface AgentStepBodyProps {
  value: NewAgentStep;
  onChange: (next: NewAgentStep) => void;
}

export function AgentStepBody({ value, onChange }: AgentStepBodyProps) {
  const [attachPrompt, onPromptSelect, insertIntoPrompt] = useCursorInsert<HTMLTextAreaElement>(
    value.prompt,
    (next) => onChange({ ...value, prompt: next }),
  );
  const tokenGuess = Math.ceil(value.prompt.length / 4);

  return (
    <div className="space-y-4">
      <div className="grid grid-cols-2 gap-3">
        <div>
          <FieldLabel>Agent type</FieldLabel>
          <select
            value={value.agent_type}
            onChange={(e) =>
              onChange({ ...value, agent_type: e.target.value as "claude_code_cli" })
            }
            className="w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input text-fg focus:outline-none focus:border-fg"
          >
            <option value="claude_code_cli">claude_code_cli</option>
          </select>
        </div>
        <div>
          <FieldLabel>Model</FieldLabel>
          <MonoTextInput
            value={value.model ?? ""}
            onChange={(e) =>
              onChange({ ...value, model: e.target.value.length > 0 ? e.target.value : null })
            }
            placeholder="default"
          />
        </div>
      </div>

      <div>
        <FieldLabel>Extra CLI args</FieldLabel>
        <ChipInput
          values={value.extra_args ?? []}
          onChange={(next) => onChange({ ...value, extra_args: next })}
          placeholder="add arg…"
          ariaLabel="Extra CLI args"
        />
      </div>

      <div>
        <FieldLabel>Prompt</FieldLabel>
        <TextAreaInput
          ref={attachPrompt}
          value={value.prompt}
          onSelect={onPromptSelect}
          onChange={(e) => onChange({ ...value, prompt: e.target.value })}
          placeholder="You are a concise summarizer…"
          spellCheck={false}
          className="min-h-[200px] bg-surface-secondary !whitespace-pre-wrap"
          style={{ whiteSpace: "pre-wrap" }}
        />
        <div className="text-[10.5px] text-fg-muted font-mono mt-1">
          ~{tokenGuess} tokens (rough estimate)
        </div>
        <TemplateChips onInsert={insertIntoPrompt} />
      </div>
    </div>
  );
}
