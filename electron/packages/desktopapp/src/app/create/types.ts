/**
 * NewWorkflow / NewStep types
 *
 * Local-only types used by the /create page to model the workflow under
 * construction. These mirror the backend `POST /api/workflows` request
 * body (see `acs/src/models/workflow.rs`). They intentionally differ from
 * the read-side `Job` / `WorkflowStep` types in `apis/types.ts`:
 *
 *  - Most fields are optional during editing.
 *  - We track step kinds as a tagged union with kind-specific fields.
 *  - Nested step lists (match.cases / match.default) hold full NewStep
 *    objects so the graph can render them as sub-chains.
 *
 * The shape is serialised verbatim (after a small `pruneEmpty` pass)
 * when the user clicks "Create Workflow".
 */

export type StepKind = "shell" | "script" | "http" | "match" | "set_var" | "agent";

export type ScriptType = "shell" | "batch" | "python" | "powershell";

export type OnFailure =
  | "abort"
  | "continue"
  | { retry: { attempts: number; backoff_ms: number } };

export interface NewStepCommon {
  id: string;
  kind: StepKind;
  on_failure?: OnFailure;
  always_run?: boolean;
  timeout_secs?: number | null;
  working_dir?: string | null;
  env_vars?: Record<string, string> | null;
  capture?: Record<string, unknown>;
}

export interface NewShellStep extends NewStepCommon {
  kind: "shell";
  command: string;
  pass_stdin?: boolean;
}

export interface NewScriptStep extends NewStepCommon {
  kind: "script";
  path: string;
  script_type: ScriptType;
  args?: string[];
  pass_stdin?: boolean;
}

export interface NewHttpStep extends NewStepCommon {
  kind: "http";
  method: string;
  url: string;
  headers?: Record<string, string>;
  body?: string | null;
  expect_status?: number[];
}

export interface NewMatchStep extends NewStepCommon {
  kind: "match";
  expr: string;
  cases: Record<string, NewStep[]>;
  default?: NewStep[];
}

export interface NewSetVarStep extends NewStepCommon {
  kind: "set_var";
  exports: Record<string, string>;
}

export interface NewAgentStep extends NewStepCommon {
  kind: "agent";
  agent_type: "claude_code_cli";
  prompt: string;
  model?: string | null;
  extra_args?: string[];
}

export type NewStep =
  | NewShellStep
  | NewScriptStep
  | NewHttpStep
  | NewMatchStep
  | NewSetVarStep
  | NewAgentStep;

export interface NewWorkflow {
  name: string;
  schedule: string;
  timezone?: string;
  schedule_mode?: "Cron" | "WaitForCompletion";
  enabled?: boolean;
  allow_concurrent?: boolean;
  on_failure?: OnFailure;
  default_input?: Record<string, unknown> | null;
  working_dir?: string | null;
  env_vars?: Record<string, string> | null;
  steps: NewStep[];
}

/**
 * Generates a short unique-ish id for new steps. The backend has no
 * requirements about step id format beyond uniqueness within the
 * workflow, but a readable prefix helps debugging.
 */
export function makeStepId(kind: StepKind): string {
  const rand = Math.random().toString(36).slice(2, 7);
  return `${kind}_${rand}`;
}

/**
 * Returns a freshly-constructed step of the given kind, pre-populated
 * with sensible defaults so the graph node renders something
 * meaningful immediately.
 */
export function makeDefaultStep(kind: StepKind): NewStep {
  const id = makeStepId(kind);
  switch (kind) {
    case "shell":
      return { id, kind, command: "echo hello", pass_stdin: false };
    case "script":
      return {
        id,
        kind,
        path: "./script.sh",
        script_type: "shell",
        args: [],
        pass_stdin: false,
      };
    case "http":
      return {
        id,
        kind,
        method: "GET",
        url: "https://example.com",
        expect_status: [200],
      };
    case "match":
      return {
        id,
        kind,
        expr: "${steps.previous.stdout}",
        cases: { ok: [] },
        default: [],
      };
    case "set_var":
      return { id, kind, exports: { MY_VAR: "value" } };
    case "agent":
      return {
        id,
        kind,
        agent_type: "claude_code_cli",
        prompt: "Summarize the previous step output.",
        extra_args: [],
      };
  }
}
