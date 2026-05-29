/**
 * stepMeta
 *
 * Per-StepKind metadata: human label, mesh persona for the node body,
 * lucide icon, and a `summarize()` helper producing the one-line text
 * shown under the kind label on each graph node.
 *
 * Centralising this keeps the graph node component generic and lets the
 * kind-picker popover render previews from the same source of truth.
 */

import type { ComponentType } from "react";
import {
  Terminal,
  FileCode2,
  Globe,
  GitBranch,
  Variable,
  Sparkles,
  type LucideProps,
} from "lucide-react";
import type { NewStep, StepKind } from "./types";

export interface StepKindMeta {
  kind: StepKind;
  label: string;
  /** Persona name for `data-mesh="<persona>"`. */
  mesh: "mist" | "lemon" | "peach" | "fog" | "ember" | "pulse";
  Icon: ComponentType<LucideProps>;
  description: string;
}

export const STEP_KIND_META: Record<StepKind, StepKindMeta> = {
  shell: {
    kind: "shell",
    label: "Shell",
    mesh: "fog",
    Icon: Terminal,
    description: "Run a shell command.",
  },
  script: {
    kind: "script",
    label: "Script",
    mesh: "mist",
    Icon: FileCode2,
    description: "Execute a script file.",
  },
  http: {
    kind: "http",
    label: "HTTP",
    mesh: "pulse",
    Icon: Globe,
    description: "Make an HTTP request.",
  },
  match: {
    kind: "match",
    label: "Match",
    mesh: "lemon",
    Icon: GitBranch,
    description: "Branch on an expression.",
  },
  set_var: {
    kind: "set_var",
    label: "Set Var",
    mesh: "peach",
    Icon: Variable,
    description: "Mutate workflow context.",
  },
  agent: {
    kind: "agent",
    label: "Agent",
    mesh: "ember",
    Icon: Sparkles,
    description: "Invoke an LLM agent.",
  },
};

export const STEP_KINDS: StepKind[] = [
  "shell",
  "script",
  "http",
  "match",
  "set_var",
  "agent",
];

function truncate(text: string, n: number): string {
  const clean = text.replace(/\s+/g, " ").trim();
  if (clean.length <= n) return clean;
  return `${clean.slice(0, n - 1)}…`;
}

export function summarizeStep(step: NewStep): string {
  switch (step.kind) {
    case "shell":
      return truncate(step.command || "(no command)", 60);
    case "script":
      return truncate(`${step.script_type} · ${step.path || "(no path)"}`, 60);
    case "http":
      return truncate(`${step.method} ${step.url || "(no url)"}`, 60);
    case "match": {
      const caseCount = Object.keys(step.cases ?? {}).length;
      const hasDefault = (step.default?.length ?? 0) > 0;
      return truncate(
        `${step.expr || "(no expr)"} · ${caseCount} case${caseCount === 1 ? "" : "s"}${
          hasDefault ? " + default" : ""
        }`,
        60,
      );
    }
    case "set_var": {
      const keys = Object.keys(step.exports ?? {});
      return truncate(keys.length ? keys.join(", ") : "(no vars)", 60);
    }
    case "agent":
      return truncate(step.prompt || "(no prompt)", 60);
  }
}
