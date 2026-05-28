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
 * RunWithOverridesModal
 *
 * Per-trigger override sheet for "Run with Overrides" on JobDetailSidebar.
 * Three orthogonal sections, each gated by a leading checkbox:
 *   1. Custom input  — JSON textarea, parsed on Submit
 *   2. Env overrides — table of key/value rows with add/remove
 *   3. Target step   — dropdown of `workflow.steps[].id` (when non-empty)
 *
 * Only the checked sections contribute to the trigger body — unchecked
 * sections are omitted so the workflow falls back to its defaults. JSON
 * parse errors and API errors surface as inline banners. On success the
 * modal closes and the parent's runs list refreshes via SSE.
 */

interface RunWithOverridesModalProps {
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

export function RunWithOverridesModal({
  workflow,
  isOpen,
  onOpenChange,
  onTriggered,
}: RunWithOverridesModalProps) {
  const { trigger, triggering, error: triggerError } = useTriggerWorkflow();

  const defaultInputPlaceholder = useMemo(() => {
    if (!workflow.default_input) return '{\n  "key": "value"\n}';
    return JSON.stringify(workflow.default_input, null, 2);
  }, [workflow.default_input]);

  const [useInput, setUseInput] = useState(false);
  const [inputText, setInputText] = useState("");
  const [parseError, setParseError] = useState<string | null>(null);

  const [useEnv, setUseEnv] = useState(false);
  const [envRows, setEnvRows] = useState<EnvRow[]>([{ key: "", value: "" }]);

  const [useTargetStep, setUseTargetStep] = useState(false);
  const stepIds = workflow.steps.map((s) => s.id);
  const [targetStep, setTargetStep] = useState<string>(stepIds[0] ?? "");

  function handleOpenChange(open: boolean) {
    if (!open) {
      // Reset on close — keeps the next open clean.
      setUseInput(false);
      setInputText("");
      setParseError(null);
      setUseEnv(false);
      setEnvRows([{ key: "", value: "" }]);
      setUseTargetStep(false);
      setTargetStep(stepIds[0] ?? "");
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
    setEnvRows((rows) => (rows.length === 1 ? [{ key: "", value: "" }] : rows.filter((_, i) => i !== index)));
  }

  const existingEnvEntries = workflow.env_vars
    ? Object.entries(workflow.env_vars)
    : [];

  return (
    <ModalOverlay
      isOpen={isOpen}
      onOpenChange={handleOpenChange}
      isDismissable={!triggering}
      className="fixed inset-0 z-50 flex items-center justify-center bg-fg/30 backdrop-blur-sm entering:animate-in entering:fade-in exiting:animate-out exiting:fade-out"
    >
      <Modal className="bg-surface border border-border rounded-card shadow-menu max-w-xl w-full max-h-[90vh] flex flex-col outline-none entering:animate-in entering:zoom-in-95 exiting:animate-out exiting:zoom-out-95">
        <Dialog className="outline-none flex flex-col gap-4 p-6 overflow-y-auto">
          <Heading slot="title" className="text-fg text-lg font-semibold">
            Run with Overrides
          </Heading>
          <p className="text-fg-muted text-sm">
            Trigger <strong className="text-fg">{workflow.name}</strong> with per-run
            input, env, and/or target-step overrides. Unchecked sections fall back
            to the workflow defaults.
          </p>

          {/* Custom input */}
          <section className="flex flex-col gap-2 p-3 bg-surface-secondary border border-border-subtle rounded-card">
            <label className="flex items-center gap-2 text-sm font-medium text-fg cursor-pointer">
              <input
                type="checkbox"
                checked={useInput}
                onChange={(e) => setUseInput(e.target.checked)}
                className="accent-brand"
              />
              Custom input
            </label>
            {useInput && (
              <>
                <textarea
                  value={inputText}
                  onChange={(e) => setInputText(e.target.value)}
                  placeholder={defaultInputPlaceholder}
                  rows={6}
                  spellCheck={false}
                  className="w-full px-3 py-2 text-xs font-mono bg-surface border border-border rounded-input outline-none focus:border-border-active focus:ring-2 focus:ring-brand-ring placeholder-fg-subtle resize-y"
                />
                {workflow.default_input && (
                  <p className="text-xs text-fg-muted">
                    Placeholder shows the workflow&apos;s <code>default_input</code>.
                  </p>
                )}
              </>
            )}
          </section>

          {/* Env overrides */}
          <section className="flex flex-col gap-2 p-3 bg-surface-secondary border border-border-subtle rounded-card">
            <label className="flex items-center gap-2 text-sm font-medium text-fg cursor-pointer">
              <input
                type="checkbox"
                checked={useEnv}
                onChange={(e) => setUseEnv(e.target.checked)}
                className="accent-brand"
              />
              Environment variable overrides
            </label>
            {useEnv && (
              <div className="flex flex-col gap-2">
                {existingEnvEntries.length > 0 && (
                  <div className="text-xs text-fg-muted flex flex-col gap-0.5">
                    <span>Currently set on the workflow (will be overridden):</span>
                    <ul className="font-mono text-fg-tertiary">
                      {existingEnvEntries.map(([k, v]) => (
                        <li key={k}>
                          {k}={v}
                        </li>
                      ))}
                    </ul>
                  </div>
                )}
                <div className="flex flex-col gap-1">
                  {envRows.map((row, i) => (
                    <div key={i} className="flex items-center gap-1">
                      <input
                        type="text"
                        value={row.key}
                        onChange={(e) => updateRow(i, { key: e.target.value })}
                        placeholder="KEY"
                        className="flex-1 px-2 py-1 text-xs font-mono bg-surface border border-border rounded-input outline-none focus:border-border-active placeholder-fg-subtle"
                      />
                      <span className="text-fg-muted">=</span>
                      <input
                        type="text"
                        value={row.value}
                        onChange={(e) => updateRow(i, { value: e.target.value })}
                        placeholder="value"
                        className="flex-1 px-2 py-1 text-xs font-mono bg-surface border border-border rounded-input outline-none focus:border-border-active placeholder-fg-subtle"
                      />
                      <button
                        type="button"
                        onClick={() => removeRow(i)}
                        aria-label="Remove row"
                        className="p-1 text-fg-muted hover:text-status-failed hover:bg-status-failed-bg rounded-input cursor-pointer"
                      >
                        <Trash2 size={14} />
                      </button>
                    </div>
                  ))}
                  <button
                    type="button"
                    onClick={addRow}
                    className="self-start inline-flex items-center gap-1 px-2 py-1 text-xs text-fg-secondary hover:text-fg hover:bg-surface-hover rounded-input cursor-pointer"
                  >
                    <Plus size={14} /> Add row
                  </button>
                </div>
              </div>
            )}
          </section>

          {/* Target step */}
          <section className="flex flex-col gap-2 p-3 bg-surface-secondary border border-border-subtle rounded-card">
            <label className="flex items-center gap-2 text-sm font-medium text-fg cursor-pointer">
              <input
                type="checkbox"
                checked={useTargetStep}
                onChange={(e) => setUseTargetStep(e.target.checked)}
                disabled={stepIds.length === 0}
                className="accent-brand disabled:opacity-50"
              />
              Target step
              {stepIds.length === 0 && (
                <span className="text-xs text-fg-muted">(workflow has no steps)</span>
              )}
            </label>
            {useTargetStep && stepIds.length > 0 && (
              <select
                value={targetStep}
                onChange={(e) => setTargetStep(e.target.value)}
                className="w-full px-3 py-2 text-sm bg-surface border border-border rounded-input outline-none focus:border-border-active cursor-pointer"
              >
                {stepIds.map((id) => (
                  <option key={id} value={id}>
                    {id}
                  </option>
                ))}
              </select>
            )}
          </section>

          {(parseError || triggerError) && (
            <div className="p-3 text-sm bg-status-failed-bg border border-status-failed-border text-status-failed rounded-card">
              {parseError ?? triggerError}
            </div>
          )}

          <div className="flex items-center justify-end gap-2 pt-2">
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
              {triggering ? "Triggering…" : "Trigger"}
            </Button>
          </div>
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
