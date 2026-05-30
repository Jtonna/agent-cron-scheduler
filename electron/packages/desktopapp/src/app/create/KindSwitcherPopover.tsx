"use client";

/**
 * KindSwitcherPopover
 *
 * Popover that lists the 5 other step kinds. Selecting one applies a
 * kind change to the current step. Heuristically warns the user when
 * the current step has non-default kind-specific fields that will be
 * reset by the switch (the common fields — id, on_failure, always_run,
 * timeout_secs, working_dir, env_vars, capture — are preserved).
 */

import { useEffect, useRef } from "react";
import { STEP_KIND_META, STEP_KINDS } from "./stepMeta";
import { makeDefaultStep, type NewStep, type StepKind } from "./types";

interface KindSwitcherPopoverProps {
  current: NewStep;
  onPick: (next: NewStep) => void;
  onClose: () => void;
}

export function KindSwitcherPopover({ current, onPick, onClose }: KindSwitcherPopoverProps) {
  const ref = useRef<HTMLDivElement>(null);
  const warn = hasNonDefaultKindFields(current);

  useEffect(() => {
    function onDown(e: MouseEvent) {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose();
    }
    function onKey(e: KeyboardEvent) {
      if (e.key === "Escape") onClose();
    }
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const handlePick = (kind: StepKind) => {
    if (kind === current.kind) {
      onClose();
      return;
    }
    onPick(applyKindChange(current, kind));
    onClose();
  };

  const others = STEP_KINDS.filter((k) => k !== current.kind);

  return (
    <div
      ref={ref}
      role="menu"
      aria-label="Switch step kind"
      className="absolute right-2 top-[42px] z-20 w-[240px] bg-surface border border-border rounded-card shadow-menu p-1.5"
    >
      {others.map((kind) => {
        const meta = STEP_KIND_META[kind];
        const Icon = meta.Icon;
        return (
          <button
            key={kind}
            type="button"
            role="menuitem"
            onClick={() => handlePick(kind)}
            className="w-full flex items-center gap-2 px-2 py-1.5 rounded-input hover:bg-surface-hover cursor-pointer text-left"
          >
            <span
              data-mesh={meta.mesh}
              className="inline-flex h-[18px] w-[18px] rounded-input items-center justify-center"
            >
              <Icon size={11} className="text-fg" strokeWidth={2.25} />
            </span>
            <span className="font-mono text-[12px] text-fg">{meta.label.toLowerCase()}</span>
          </button>
        );
      })}
      {warn && (
        <div className="mt-1 border-t border-border pt-2 px-2 pb-1 text-[10.5px] text-fg-muted leading-snug">
          <b className="text-fg font-medium">Heads up.</b> Changing kind will reset
          kind-specific fields. Common fields (id, on_failure, timeout, env_vars, capture)
          are preserved.
        </div>
      )}
    </div>
  );
}

/**
 * Returns true when the current step has kind-specific fields that
 * differ from a freshly-constructed default of the same kind. Used to
 * decide whether the kind-change warning is worth showing.
 */
function hasNonDefaultKindFields(step: NewStep): boolean {
  switch (step.kind) {
    case "shell":
      return (step.command ?? "") !== "" && step.command !== "echo hello";
    case "script":
      return (
        (step.path ?? "") !== "" && step.path !== "./script.sh"
      ) || (step.args?.length ?? 0) > 0;
    case "http":
      return (
        step.url !== "" &&
        step.url !== "https://example.com"
      ) ||
        Object.keys(step.headers ?? {}).length > 0 ||
        !!step.body;
    case "match":
      return step.expr.length > 0 || Object.keys(step.cases ?? {}).length > 1;
    case "set_var":
      return Object.keys(step.exports ?? {}).length > 0 && !isDefaultExports(step.exports);
    case "agent":
      return (
        step.prompt.length > 0 &&
        step.prompt !== "Summarize the previous step output."
      );
  }
}

function isDefaultExports(e: Record<string, string>): boolean {
  const keys = Object.keys(e);
  return keys.length === 1 && keys[0] === "MY_VAR";
}

/**
 * Produces a fresh step of the new `kind` carrying over the common
 * fields from `current`. Kind-specific fields come from `makeDefaultStep`.
 */
export function applyKindChange(current: NewStep, kind: StepKind): NewStep {
  const fresh = makeDefaultStep(kind);
  const merged: NewStep = {
    ...fresh,
    id: current.id,
    on_failure: current.on_failure,
    always_run: current.always_run,
    timeout_secs: current.timeout_secs,
    working_dir: current.working_dir,
    env_vars: current.env_vars,
    capture: current.capture,
  } as NewStep;
  return merged;
}
