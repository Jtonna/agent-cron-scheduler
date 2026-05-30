/**
 * workflowSerialization
 *
 * Bridges between the editor's local `NewWorkflow` shape and the two
 * server-facing shapes:
 *
 *   - `jobToNewWorkflow` — read direction. Converts a server-shape `Job`
 *     (the workflow read model from `apis/types.ts`) into the local
 *     `NewWorkflow` the editor manipulates. Strips server-managed
 *     fields (`id`, `version`, `created_at`, `updated_at`,
 *     `last_run_*`, `next_run_at`, `is_favorited`) and normalises
 *     null-vs-undefined for optional fields. The step list is cast
 *     through `unknown` because the read-side `WorkflowStep` union
 *     doesn't enumerate all kinds the backend actually supports
 *     (script, match, set_var) — they round-trip as opaque structured
 *     data.
 *
 *   - `serialiseWorkflow` — write direction. Strips empty / undefined
 *     fields out of the `NewWorkflow` before sending it to the backend.
 *     The backend's struct uses `#[serde(default)]` so absent fields are
 *     fine; sending `null` for some fields would actually conflict with
 *     the type. PATCH semantics: this whole payload replaces the
 *     workflow's mutable fields on the server.
 *
 * Kept in its own module so `WorkflowGraphEditor` can stay focused on
 * canvas state and these conversion helpers can be tested in isolation
 * if/when the conversion table grows.
 */

import type { Job } from "@/apis/types";
import type { NewStep, NewWorkflow } from "./types";

export function jobToNewWorkflow(job: Job): NewWorkflow {
  const next: NewWorkflow = {
    name: job.name,
    schedule: job.schedule,
    steps: job.steps as unknown as NewStep[],
  };
  if (job.timezone) next.timezone = job.timezone;
  if (job.schedule_mode === "Cron" || job.schedule_mode === "WaitForCompletion") {
    next.schedule_mode = job.schedule_mode;
  }
  if (typeof job.enabled === "boolean") next.enabled = job.enabled;
  if (typeof job.allow_concurrent === "boolean") next.allow_concurrent = job.allow_concurrent;
  if (job.on_failure === "abort" || job.on_failure === "continue") {
    next.on_failure = job.on_failure;
  }
  if (job.default_input) next.default_input = job.default_input;
  if (job.working_dir) next.working_dir = job.working_dir;
  if (job.env_vars) next.env_vars = job.env_vars;
  return next;
}

export function serialiseWorkflow(workflow: NewWorkflow): Record<string, unknown> {
  const body: Record<string, unknown> = {
    name: workflow.name.trim(),
    schedule: workflow.schedule.trim(),
    steps: workflow.steps,
  };
  if (workflow.timezone) body.timezone = workflow.timezone;
  if (workflow.schedule_mode) body.schedule_mode = workflow.schedule_mode;
  if (typeof workflow.enabled === "boolean") body.enabled = workflow.enabled;
  if (typeof workflow.allow_concurrent === "boolean") {
    body.allow_concurrent = workflow.allow_concurrent;
  }
  if (workflow.on_failure) body.on_failure = workflow.on_failure;
  if (workflow.default_input) body.default_input = workflow.default_input;
  if (workflow.working_dir) body.working_dir = workflow.working_dir;
  if (workflow.env_vars) body.env_vars = workflow.env_vars;
  return body;
}
