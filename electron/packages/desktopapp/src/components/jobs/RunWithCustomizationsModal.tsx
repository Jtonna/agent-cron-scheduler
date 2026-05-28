"use client";

import { useMemo, useState } from "react";
import {
  Dialog,
  Heading,
  Modal,
  ModalOverlay,
} from "react-aria-components";
import { Plus, Trash2 } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { useTriggerWorkflow } from "@/apis/useTriggerWorkflow";
import type { Job, TriggerParams, WorkflowTriggerResponse } from "@/apis/types";

/**
 * RunWithCustomizationsModal
 *
 * Per-trigger customization sheet opened from JobDetailSidebar's
 * "Run with Customizations" action. Three orthogonal sections, each gated
 * by a leading checkbox so the user can opt-in to overriding just the
 * pieces they care about:
 *
 *   1. Custom input  — JSON textarea, prefilled from `workflow.default_input`
 *                      so the user is editing a sensible starting point
 *                      rather than staring at a blank box.
 *   2. Env overrides — table of key/value rows; prefilled with the
 *                      workflow's existing env vars (editable inline) plus
 *                      a blank row for additions.
 *   3. Target step   — dropdown of steps showing both id and kind. Only
 *                      `shell` steps are targetable (other kinds are
 *                      silently ignored by the backend); non-targetable
 *                      kinds are listed as disabled options so the user
 *                      can see them but can't pick them.
 *
 * Unchecked sections are omitted from the request body so the workflow
 * falls back to its defaults. JSON parse errors and API errors surface
 * as inline banners. On success the modal closes and the parent's runs
 * list refreshes via SSE.
 *
 * The backend API still calls these "overrides" — the UI rename to
 * "customizations" is purely user-facing.
 */

interface RunWithCustomizationsModalProps {
  workflow: Job;
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  /** Called after a successful trigger with the new run's response. */
  onTriggered?: (response: WorkflowTriggerResponse) => void;
}

interface EnvRow {
  key: string;
  value: string;
}

/** Step kinds the backend will honour for `target_step`. */
const TARGETABLE_KINDS = new Set(["shell"]);

function isTargetable(kind: string): boolean {
  return TARGETABLE_KINDS.has(kind);
}

function defaultEnvRows(workflow: Job): EnvRow[] {
  const existing = workflow.env_vars ? Object.entries(workflow.env_vars) : [];
  if (existing.length === 0) return [{ key: "", value: "" }];
  return existing.map(([key, value]) => ({ key, value }));
}

function defaultInputText(workflow: Job): string {
  if (!workflow.default_input) return "";
  return JSON.stringify(workflow.default_input, null, 2);
}

function firstTargetableStepId(workflow: Job): string {
  return workflow.steps.find((s) => isTargetable(s.kind))?.id ?? "";
}

export function RunWithCustomizationsModal({
  workflow,
  isOpen,
  onOpenChange,
  onTriggered,
}: RunWithCustomizationsModalProps) {
  const { trigger, triggering, error: triggerError } = useTriggerWorkflow();

  const inputPlaceholder = useMemo(
    () => '{\n  "key": "value"\n}',
    [],
  );

  const [useInput, setUseInput] = useState(false);
  const [inputText, setInputText] = useState(() => defaultInputText(workflow));
  const [parseError, setParseError] = useState<string | null>(null);

  const [useEnv, setUseEnv] = useState(false);
  const [envRows, setEnvRows] = useState<EnvRow[]>(() => defaultEnvRows(workflow));

  const targetableSteps = useMemo(
    () => workflow.steps.filter((s) => isTargetable(s.kind)),
    [workflow.steps],
  );
  const hasTargetableSteps = targetableSteps.length > 0;

  const [useTargetStep, setUseTargetStep] = useState(false);
  const [targetStep, setTargetStep] = useState<string>(() =>
    firstTargetableStepId(workflow),
  );

  // Note: state is re-seeded only on close (see handleOpenChange). The
  // modal is mounted per-page and the workflow identity doesn't change
  // while open — if that ever changes, the parent should `key={workflow.id}`
  // this component to force a fresh mount.

  function handleOpenChange(open: boolean) {
    if (!open) {
      // Reset to the workflow's defaults — keeps the next open clean.
      setUseInput(false);
      setInputText(defaultInputText(workflow));
      setParseError(null);
      setUseEnv(false);
      setEnvRows(defaultEnvRows(workflow));
      setUseTargetStep(false);
      setTargetStep(firstTargetableStepId(workflow));
    }
    onOpenChange(open);
  }

  async function handleSubmit() {
    setParseError(null);
    const params: TriggerParams = {};

    if (useInput) {
      if (!inputText.trim()) {
        setParseError("Custom input is empty — uncheck the box or enter JSON.");
        return;
      }
      try {
        params.input = JSON.parse(inputText);
      } catch (err) {
        setParseError(
          err instanceof Error ? `Invalid JSON: ${err.message}` : "Invalid JSON",
        );
        return;
      }
    }

    if (useEnv) {
      const env: Record<string, string> = {};
      for (const row of envRows) {
        const k = row.key.trim();
        if (!k) continue;
        env[k] = row.value;
      }
      if (Object.keys(env).length === 0) {
        setParseError("Env overrides enabled but no keys provided.");
        return;
      }
      params.env = env;
    }

    if (useTargetStep) {
      if (!targetStep) {
        setParseError("Target step enabled but no step selected.");
        return;
      }
      params.target_step = targetStep;
    }

    try {
      const response = await trigger(workflow.id, params);
      onTriggered?.(response);
      handleOpenChange(false);
    } catch {
      // Error surfaces via triggerError below.
    }
  }

  function updateRow(index: number, patch: Partial<EnvRow>) {
    setEnvRows((rows) => rows.map((r, i) => (i === index ? { ...r, ...patch } : r)));
  }

  function addRow() {
    setEnvRows((rows) => [...rows, { key: "", value: "" }]);
  }

  function removeRow(index: number) {
    setEnvRows((rows) =>
      rows.length === 1 ? [{ key: "", value: "" }] : rows.filter((_, i) => i !== index),
    );
  }

  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      isDismissable={!triggering}
      className="fixed inset-0 z-50 flex items-center justify-center bg-fg/30 backdrop-blur-sm entering:animate-in entering:fade-in exiting:animate-out exiting:fade-out"
    >
      <Modal className="bg-surface border border-border rounded-card shadow-menu max-w-2xl w-full max-h-[90vh] flex flex-col outline-none entering:animate-in entering:zoom-in-95 exiting:animate-out exiting:zoom-out-95">
        <Dialog className="outline-none flex flex-col">
          {/* Header */}
          <div className="px-6 pt-6 pb-4 border-b border-border-subtle">
            <Heading slot="title" className="text-fg text-lg font-semibold">
              Run with Customizations
            </Heading>
            <p className="text-fg-muted text-sm mt-1">
              Trigger <strong className="text-fg">{workflow.name}</strong> once with
              tweaked input, env, or a single target step. Unchecked sections fall
              back to the workflow defaults.
            </p>
          </div>

          {/* Scrollable body */}
          <div className="flex flex-col gap-5 px-6 py-5 overflow-y-auto">
            {/* Custom input */}
            <section className="flex flex-col gap-2">
              <label className="flex items-start gap-2 text-sm font-medium text-fg cursor-pointer">
                <input
                  type="checkbox"
                  checked={useInput}
                  onChange={(e) => setUseInput(e.target.checked)}
                  className="mt-0.5 accent-brand"
                />
                <span className="flex flex-col gap-0.5">
                  <span>Custom input</span>
                  <span className="text-xs font-normal text-fg-muted">
                    JSON object passed to the run.
                    {workflow.default_input
                      ? " Prefilled from the workflow's default — edit freely."
                      : " This workflow has no default input."}
                  </span>
                </span>
              </label>
              {useInput && (
                <div className="pl-6">
                  <textarea
                    value={inputText}
                    onChange={(e) => setInputText(e.target.value)}
                    placeholder={inputPlaceholder}
                    rows={6}
                    spellCheck={false}
                    className="w-full px-3 py-2 text-xs font-mono bg-surface-secondary border border-border rounded-input outline-none focus:border-border-active focus:ring-2 focus:ring-brand-ring placeholder-fg-subtle resize-y"
                  />
                </div>
              )}
            </section>

            {/* Env overrides */}
            <section className="flex flex-col gap-2">
              <label className="flex items-start gap-2 text-sm font-medium text-fg cursor-pointer">
                <input
                  type="checkbox"
                  checked={useEnv}
                  onChange={(e) => setUseEnv(e.target.checked)}
                  className="mt-0.5 accent-brand"
                />
                <span className="flex flex-col gap-0.5">
                  <span>Environment variables</span>
                  <span className="text-xs font-normal text-fg-muted">
                    {workflow.env_vars
                      ? "Prefilled with the workflow's existing vars — edit values or add new keys."
                      : "Add ad-hoc env vars for this run only."}
                  </span>
                </span>
              </label>
              {useEnv && (
                <div className="flex flex-col gap-1.5 pl-6">
                  {envRows.map((row, i) => (
                    <div key={i} className="flex items-center gap-1">
                      <input
                        type="text"
                        value={row.key}
                        onChange={(e) => updateRow(i, { key: e.target.value })}
                        placeholder="KEY"
                        className="flex-1 px-2 py-1.5 text-xs font-mono bg-surface-secondary border border-border rounded-input outline-none focus:border-border-active placeholder-fg-subtle"
                      />
                      <span className="text-fg-muted text-xs">=</span>
                      <input
                        type="text"
                        value={row.value}
                        onChange={(e) => updateRow(i, { value: e.target.value })}
                        placeholder="value"
                        className="flex-1 px-2 py-1.5 text-xs font-mono bg-surface-secondary border border-border rounded-input outline-none focus:border-border-active placeholder-fg-subtle"
                      />
                      <button
                        type="button"
                        onClick={() => removeRow(i)}
                        aria-label="Remove row"
                        className="p-1.5 text-fg-muted hover:text-status-failed hover:bg-status-failed-bg rounded-input cursor-pointer transition-colors"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={addRow}
                    className="self-start inline-flex items-center gap-1 px-2 py-1 mt-0.5 text-xs text-fg-secondary hover:text-fg hover:bg-surface-hover rounded-input cursor-pointer transition-colors"
                  >
                    <Plus size={14} /> Add row
                  </button>
                </div>
              )}
            </section>

            {/* Target step */}
            <section className="flex flex-col gap-2">
              <label className="flex items-start gap-2 text-sm font-medium text-fg cursor-pointer">
                <input
                  type="checkbox"
                  checked={useTargetStep}
                  onChange={(e) => setUseTargetStep(e.target.checked)}
                  disabled={!hasTargetableSteps}
                  className="mt-0.5 accent-brand disabled:opacity-50"
                />
                <span className="flex flex-col gap-0.5">
                  <span className={hasTargetableSteps ? "" : "text-fg-muted"}>
                    Run a single step
                  </span>
                  <span className="text-xs font-normal text-fg-muted">
                    {hasTargetableSteps
                      ? "Skip the rest of the workflow and run just the selected step."
                      : workflow.steps.length === 0
                      ? "Workflow has no steps."
                      : "Only shell steps can be targeted directly."}
                  </span>
                </span>
              </label>
              {useTargetStep && hasTargetableSteps && (
                <div className="pl-6">
                  <select
                    value={targetStep}
                    onChange={(e) => setTargetStep(e.target.value)}
                    className="w-full px-3 py-2 text-sm bg-surface-secondary border border-border rounded-input outline-none focus:border-border-active cursor-pointer"
                  >
                    {workflow.steps.map((step) => {
                      const targetable = isTargetable(step.kind);
                      return (
                        <option
                          key={step.id}
                          value={step.id}
                          disabled={!targetable}
                        >
                          {step.id} — {step.kind}
                          {!targetable ? " (not targetable)" : ""}
                        </option>
                      );
                    })}
                  </select>
                </div>
              )}
            </section>

            {(parseError || triggerError) && (
              <div className="p-3 text-sm bg-status-failed-bg border border-status-failed-border text-status-failed rounded-card">
                {parseError ?? triggerError}
              </div>
            )}
          </div>

          {/* Footer */}
          <div className="flex items-center justify-end gap-2 px-6 py-4 border-t border-border-subtle bg-surface-secondary rounded-b-card">
            <Button
              intent="ghost"
              size="sm"
              onPress={() => handleOpenChange(false)}
              isDisabled={triggering}
            >
              Cancel
            </Button>
            <Button
              intent="primary"
              size="sm"
              onPress={handleSubmit}
              isDisabled={triggering}
            >
              {triggering ? "Triggering…" : "Run workflow"}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
