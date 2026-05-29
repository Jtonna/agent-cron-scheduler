"use client";

/**
 * StepEditorPanel
 *
 * Slide-in side panel that edits one step. The panel reads the step
 * out of the parent state via `path`, renders kind-specific inputs at
 * the top, and a collapsible "Advanced" section for the shared
 * (always_run, timeout, working_dir, env_vars) fields.
 *
 * All edits flow back to the parent via `onChange`; the parent owns
 * canonical state and re-derives the graph.
 */

import { useState } from "react";
import { X, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { Toggle } from "@/components/ui/Toggle";
import type {
  NewStep,
  NewShellStep,
  NewScriptStep,
  NewHttpStep,
  NewMatchStep,
  NewSetVarStep,
  NewAgentStep,
  ScriptType,
} from "./types";
import { STEP_KIND_META } from "./stepMeta";

interface StepEditorPanelProps {
  step: NewStep;
  path: string;
  onChange: (next: NewStep) => void;
  onClose: () => void;
  onDelete: () => void;
  /** Whether the deletion is allowed (false for the last remaining top-level step). */
  canDelete: boolean;
}

/* ── Small primitives used throughout the panel ── */

function FieldLabel({ children }: { children: React.ReactNode }) {
  return <label className="text-eyebrow !text-fg-tertiary block mb-1.5">{children}</label>;
}

function TextInput(props: React.InputHTMLAttributes<HTMLInputElement>) {
  return (
    <input
      {...props}
      className={[
        "w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input",
        "text-fg placeholder:text-fg-subtle focus:outline-none focus:border-fg",
        props.className ?? "",
      ].join(" ")}
    />
  );
}

function TextAreaInput(props: React.TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return (
    <textarea
      {...props}
      className={[
        "w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input font-mono",
        "text-fg placeholder:text-fg-subtle focus:outline-none focus:border-fg resize-y min-h-[80px]",
        props.className ?? "",
      ].join(" ")}
    />
  );
}

function SelectInput(props: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      {...props}
      className={[
        "w-full px-2.5 py-1.5 text-sm bg-surface border border-border rounded-input",
        "text-fg focus:outline-none focus:border-fg",
        props.className ?? "",
      ].join(" ")}
    />
  );
}

/* ── Kind-specific editors ── */

function ShellEditor({
  step,
  onChange,
}: {
  step: NewShellStep;
  onChange: (next: NewShellStep) => void;
}) {
  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Command</FieldLabel>
        <TextAreaInput
          value={step.command}
          onChange={(e) => onChange({ ...step, command: e.target.value })}
          placeholder="echo $WORKFLOW_NAME"
        />
      </div>
      <div className="flex items-center justify-between">
        <span className="text-sm text-fg-secondary">Pipe stdin</span>
        <Toggle
          checked={step.pass_stdin ?? false}
          onChange={(v) => onChange({ ...step, pass_stdin: v })}
          ariaLabel="Pipe stdin"
        />
      </div>
    </div>
  );
}

function ScriptEditor({
  step,
  onChange,
}: {
  step: NewScriptStep;
  onChange: (next: NewScriptStep) => void;
}) {
  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Script path</FieldLabel>
        <TextInput
          value={step.path}
          onChange={(e) => onChange({ ...step, path: e.target.value })}
          placeholder="./scripts/deploy.sh"
        />
      </div>
      <div>
        <FieldLabel>Script type</FieldLabel>
        <SelectInput
          value={step.script_type}
          onChange={(e) =>
            onChange({ ...step, script_type: e.target.value as ScriptType })
          }
        >
          <option value="shell">shell</option>
          <option value="batch">batch</option>
          <option value="python">python</option>
          <option value="powershell">powershell</option>
        </SelectInput>
      </div>
      <div>
        <FieldLabel>Args (one per line)</FieldLabel>
        <TextAreaInput
          value={(step.args ?? []).join("\n")}
          onChange={(e) =>
            onChange({
              ...step,
              args: e.target.value.split("\n").filter((s) => s.length > 0),
            })
          }
          placeholder="--verbose"
        />
      </div>
      <div className="flex items-center justify-between">
        <span className="text-sm text-fg-secondary">Pipe stdin</span>
        <Toggle
          checked={step.pass_stdin ?? false}
          onChange={(v) => onChange({ ...step, pass_stdin: v })}
          ariaLabel="Pipe stdin"
        />
      </div>
    </div>
  );
}

function HttpEditor({
  step,
  onChange,
}: {
  step: NewHttpStep;
  onChange: (next: NewHttpStep) => void;
}) {
  return (
    <div className="space-y-4">
      <div className="grid grid-cols-[100px_1fr] gap-2">
        <div>
          <FieldLabel>Method</FieldLabel>
          <SelectInput
            value={step.method}
            onChange={(e) => onChange({ ...step, method: e.target.value })}
          >
            {["GET", "POST", "PUT", "PATCH", "DELETE"].map((m) => (
              <option key={m} value={m}>
                {m}
              </option>
            ))}
          </SelectInput>
        </div>
        <div>
          <FieldLabel>URL</FieldLabel>
          <TextInput
            value={step.url}
            onChange={(e) => onChange({ ...step, url: e.target.value })}
            placeholder="https://api.example.com/v1/ping"
          />
        </div>
      </div>
      <div>
        <FieldLabel>Body (JSON or text)</FieldLabel>
        <TextAreaInput
          value={step.body ?? ""}
          onChange={(e) =>
            onChange({
              ...step,
              body: e.target.value.length ? e.target.value : null,
            })
          }
          placeholder='{"key":"value"}'
        />
      </div>
      <div>
        <FieldLabel>Expect status (comma-separated)</FieldLabel>
        <TextInput
          value={(step.expect_status ?? []).join(", ")}
          onChange={(e) =>
            onChange({
              ...step,
              expect_status: e.target.value
                .split(",")
                .map((s) => parseInt(s.trim(), 10))
                .filter((n) => !Number.isNaN(n)),
            })
          }
          placeholder="200, 204"
        />
      </div>
    </div>
  );
}

function MatchEditor({
  step,
  onChange,
}: {
  step: NewMatchStep;
  onChange: (next: NewMatchStep) => void;
}) {
  const [newCase, setNewCase] = useState("");
  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Match expression</FieldLabel>
        <TextInput
          value={step.expr}
          onChange={(e) => onChange({ ...step, expr: e.target.value })}
          placeholder="${steps.previous.stdout}"
        />
      </div>
      <div>
        <FieldLabel>Cases</FieldLabel>
        <div className="space-y-1">
          {Object.keys(step.cases ?? {}).map((caseKey) => (
            <div
              key={caseKey}
              className="flex items-center justify-between px-2.5 py-1.5 border border-border rounded-input"
            >
              <span className="text-sm font-mono text-fg">{caseKey}</span>
              <span className="text-xs text-fg-subtle">
                {(step.cases[caseKey]?.length ?? 0)} step
                {(step.cases[caseKey]?.length ?? 0) === 1 ? "" : "s"}
              </span>
            </div>
          ))}
        </div>
        <div className="flex gap-2 mt-2">
          <TextInput
            value={newCase}
            onChange={(e) => setNewCase(e.target.value)}
            placeholder="ok"
          />
          <Button
            intent="secondary"
            size="sm"
            shape="rounded"
            onPress={() => {
              if (!newCase.trim() || step.cases[newCase]) return;
              onChange({
                ...step,
                cases: { ...step.cases, [newCase.trim()]: [] },
              });
              setNewCase("");
            }}
          >
            Add case
          </Button>
        </div>
      </div>
      <div className="text-xs text-fg-subtle">
        Nested case-step editing is intentionally minimal in the prototype — add steps inside cases via the graph toolbar.
      </div>
    </div>
  );
}

function SetVarEditor({
  step,
  onChange,
}: {
  step: NewSetVarStep;
  onChange: (next: NewSetVarStep) => void;
}) {
  const entries = Object.entries(step.exports ?? {});
  return (
    <div className="space-y-3">
      <FieldLabel>Exports</FieldLabel>
      {entries.map(([k, v], i) => (
        <div key={`${k}-${i}`} className="grid grid-cols-[120px_1fr_auto] gap-2">
          <TextInput
            value={k}
            onChange={(e) => {
              const next = { ...step.exports };
              delete next[k];
              next[e.target.value] = v;
              onChange({ ...step, exports: next });
            }}
            placeholder="VAR_NAME"
          />
          <TextInput
            value={v}
            onChange={(e) =>
              onChange({ ...step, exports: { ...step.exports, [k]: e.target.value } })
            }
            placeholder="value"
          />
          <button
            type="button"
            className="text-fg-subtle hover:text-fg px-2"
            onClick={() => {
              const next = { ...step.exports };
              delete next[k];
              onChange({ ...step, exports: next });
            }}
          >
            <Trash2 size={14} />
          </button>
        </div>
      ))}
      <Button
        intent="secondary"
        size="sm"
        shape="rounded"
        onPress={() =>
          onChange({
            ...step,
            exports: { ...step.exports, [`VAR_${entries.length + 1}`]: "" },
          })
        }
      >
        Add export
      </Button>
    </div>
  );
}

function AgentEditor({
  step,
  onChange,
}: {
  step: NewAgentStep;
  onChange: (next: NewAgentStep) => void;
}) {
  return (
    <div className="space-y-4">
      <div>
        <FieldLabel>Agent type</FieldLabel>
        <SelectInput
          value={step.agent_type}
          onChange={(e) =>
            onChange({ ...step, agent_type: e.target.value as "claude_code_cli" })
          }
        >
          <option value="claude_code_cli">claude_code_cli</option>
        </SelectInput>
      </div>
      <div>
        <FieldLabel>Prompt</FieldLabel>
        <TextAreaInput
          value={step.prompt}
          onChange={(e) => onChange({ ...step, prompt: e.target.value })}
          placeholder="Summarize the latest weather data."
          className="min-h-[140px]"
        />
      </div>
      <div>
        <FieldLabel>Model (optional)</FieldLabel>
        <TextInput
          value={step.model ?? ""}
          onChange={(e) =>
            onChange({ ...step, model: e.target.value.length ? e.target.value : null })
          }
          placeholder="claude-opus-4"
        />
      </div>
      <div>
        <FieldLabel>Extra CLI args (one per line)</FieldLabel>
        <TextAreaInput
          value={(step.extra_args ?? []).join("\n")}
          onChange={(e) =>
            onChange({
              ...step,
              extra_args: e.target.value.split("\n").filter((s) => s.length > 0),
            })
          }
          placeholder="--no-interactive"
        />
      </div>
    </div>
  );
}

export function StepEditorPanel({
  step,
  path,
  onChange,
  onClose,
  onDelete,
  canDelete,
}: StepEditorPanelProps) {
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const meta = STEP_KIND_META[step.kind];
  const Icon = meta.Icon;

  return (
    <aside className="w-[420px] shrink-0 border-l border-border bg-surface flex flex-col h-full">
      <header className="px-5 py-4 border-b border-border-subtle flex items-center justify-between">
        <div className="flex items-center gap-2.5">
          <span
            data-mesh={meta.mesh}
            className="inline-flex h-7 w-7 rounded-pill items-center justify-center"
          >
            <Icon size={14} className="text-fg" strokeWidth={2.25} />
          </span>
          <div>
            <div className="text-eyebrow">{meta.label}</div>
            <div className="text-sm font-mono text-fg">{step.id}</div>
          </div>
        </div>
        <button
          type="button"
          onClick={onClose}
          className="text-fg-subtle hover:text-fg p-1.5 rounded-input"
          aria-label="Close editor"
        >
          <X size={16} />
        </button>
      </header>

      <div className="flex-1 overflow-y-auto px-5 py-5 space-y-5">
        <div>
          <FieldLabel>Step id</FieldLabel>
          <TextInput
            value={step.id}
            onChange={(e) => onChange({ ...step, id: e.target.value } as NewStep)}
          />
        </div>

        {step.kind === "shell" && (
          <ShellEditor step={step} onChange={(next) => onChange(next)} />
        )}
        {step.kind === "script" && (
          <ScriptEditor step={step} onChange={(next) => onChange(next)} />
        )}
        {step.kind === "http" && (
          <HttpEditor step={step} onChange={(next) => onChange(next)} />
        )}
        {step.kind === "match" && (
          <MatchEditor step={step} onChange={(next) => onChange(next)} />
        )}
        {step.kind === "set_var" && (
          <SetVarEditor step={step} onChange={(next) => onChange(next)} />
        )}
        {step.kind === "agent" && (
          <AgentEditor step={step} onChange={(next) => onChange(next)} />
        )}

        <div className="border-t border-border-subtle pt-4">
          <button
            type="button"
            onClick={() => setAdvancedOpen((v) => !v)}
            className="text-eyebrow !text-fg-tertiary hover:!text-fg cursor-pointer"
          >
            {advancedOpen ? "▾" : "▸"} Advanced
          </button>
          {advancedOpen && (
            <div className="mt-4 space-y-4">
              <div className="flex items-center justify-between">
                <span className="text-sm text-fg-secondary">Always run</span>
                <Toggle
                  checked={step.always_run ?? false}
                  onChange={(v) => onChange({ ...step, always_run: v } as NewStep)}
                  ariaLabel="Always run"
                />
              </div>
              <div>
                <FieldLabel>Timeout (seconds)</FieldLabel>
                <TextInput
                  type="number"
                  value={step.timeout_secs ?? ""}
                  onChange={(e) =>
                    onChange({
                      ...step,
                      timeout_secs: e.target.value.length
                        ? parseInt(e.target.value, 10)
                        : null,
                    } as NewStep)
                  }
                  placeholder="60"
                />
              </div>
              <div>
                <FieldLabel>Working directory</FieldLabel>
                <TextInput
                  value={step.working_dir ?? ""}
                  onChange={(e) =>
                    onChange({
                      ...step,
                      working_dir: e.target.value.length ? e.target.value : null,
                    } as NewStep)
                  }
                  placeholder="/tmp"
                />
              </div>
            </div>
          )}
        </div>

        <div className="text-[10px] font-mono text-fg-subtle pt-2">path: {path}</div>
      </div>

      <footer className="px-5 py-3 border-t border-border-subtle flex justify-between items-center">
        <button
          type="button"
          onClick={onDelete}
          disabled={!canDelete}
          className={[
            "inline-flex items-center gap-1.5 text-sm",
            canDelete
              ? "text-status-failed hover:opacity-80 cursor-pointer"
              : "text-fg-subtle cursor-not-allowed",
          ].join(" ")}
        >
          <Trash2 size={14} /> Delete step
        </button>
        <Button intent="secondary" size="sm" shape="pill" onPress={onClose}>
          Done
        </Button>
      </footer>
    </aside>
  );
}
