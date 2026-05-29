"use client";

/**
 * WorkflowGraphEditor
 *
 * The interactive editor at the heart of /create AND /workflows/[id]/edit.
 * Owns the `NewWorkflow` state, renders:
 *   - the WorkflowHeaderCard (name / schedule / timezone / enabled)
 *   - a reactflow canvas with dagre-laid-out step nodes
 *   - a slide-in StepEditorPanel when a node is selected
 *   - a toolbar with "Add step" (via KindPicker) + a primary submit button
 *
 * The component runs in one of two modes via discriminated-union props:
 *   - `mode: "create"` — seeds with a fresh `NewWorkflow`, submits via
 *     `useCreateWorkflow`, navigates to the newly-created workflow's
 *     detail page on success.
 *   - `mode: "edit"` — seeds from an existing `Workflow` (the `Job` read
 *     shape from `apis/types.ts`) after stripping server-only fields,
 *     submits via `useUpdateWorkflow`, navigates back to the same
 *     workflow's detail page on success.
 *
 * The graph is derived from `state.steps` every render; updates flow
 * through `getStepAtPath` / `updateStepAtPath` / `deleteStepAtPath` /
 * `insertStepAfter` helpers in `graph.ts`.
 */

import { useCallback, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Background,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  type Node,
  type NodeMouseHandler,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { Plus, Workflow as WorkflowIcon } from "lucide-react";
import { Button } from "@/components/ui/Button";
import { useCreateWorkflow } from "@/apis/useCreateWorkflow";
import { useUpdateWorkflow } from "@/apis/useUpdateWorkflow";
import type { Job } from "@/apis/types";
import { WorkflowHeaderCard } from "./WorkflowHeaderCard";
import { StepEditorPanel } from "./StepEditorPanel";
import { KindPicker } from "./KindPicker";
import { StepNode } from "./StepNode";
import {
  buildGraph,
  deleteStepAtPath,
  getStepAtPath,
  insertStepAfter,
  layoutGraph,
  updateStepAtPath,
  type StepNodeData,
} from "./graph";
import { makeDefaultStep } from "./types";
import type { NewStep, NewWorkflow, StepKind } from "./types";

const nodeTypes = { step: StepNode };

export type WorkflowGraphEditorProps =
  | { mode: "create"; initialWorkflow: NewWorkflow }
  | { mode: "edit"; workflowId: string; initialWorkflow: Job };

export function WorkflowGraphEditor(props: WorkflowGraphEditorProps) {
  return (
    <ReactFlowProvider>
      <WorkflowGraphEditorInner {...props} />
    </ReactFlowProvider>
  );
}

function WorkflowGraphEditorInner(props: WorkflowGraphEditorProps) {
  const router = useRouter();
  const isEdit = props.mode === "edit";

  const seed = useMemo<NewWorkflow>(
    () =>
      props.mode === "edit"
        ? jobToNewWorkflow(props.initialWorkflow)
        : props.initialWorkflow,
    // We intentionally seed only once — the prop is the initial value, not a
    // controlled mirror. Re-seeding on every fetch would clobber in-flight
    // edits (the same reason the old JSON textarea used an `initialized` flag).
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const [workflow, setWorkflow] = useState<NewWorkflow>(seed);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const { create, creating, error: createError } = useCreateWorkflow();
  const editWorkflowId = props.mode === "edit" ? props.workflowId : "";
  const {
    update,
    updating,
    error: updateError,
  } = useUpdateWorkflow(editWorkflowId);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const busy = isEdit ? updating : creating;
  const serverError = isEdit ? updateError : createError;

  const { nodes, edges } = useMemo(() => {
    const built = buildGraph(workflow.steps);
    const positioned = layoutGraph(built.nodes, built.edges);
    return { nodes: positioned, edges: built.edges };
  }, [workflow.steps]);

  const selectedStep = selectedPath ? getStepAtPath(workflow.steps, selectedPath) : null;

  const handleNodeClick: NodeMouseHandler = useCallback((_event, node: Node) => {
    const data = node.data as StepNodeData;
    setSelectedPath(data.path);
  }, []);

  const handleStepChange = useCallback(
    (next: NewStep) => {
      if (!selectedPath) return;
      setWorkflow((prev) => ({
        ...prev,
        steps: updateStepAtPath(prev.steps, selectedPath, next),
      }));
    },
    [selectedPath],
  );

  const handleStepDelete = useCallback(() => {
    if (!selectedPath) return;
    setWorkflow((prev) => ({
      ...prev,
      steps: deleteStepAtPath(prev.steps, selectedPath),
    }));
    setSelectedPath(null);
  }, [selectedPath]);

  const handleAddStep = useCallback(
    (kind: StepKind) => {
      const newStep = makeDefaultStep(kind);
      setWorkflow((prev) => ({
        ...prev,
        steps: insertStepAfter(prev.steps, selectedPath, newStep),
      }));
      setPickerOpen(false);
    },
    [selectedPath],
  );

  const handleSubmit = useCallback(async () => {
    setSubmissionError(null);
    try {
      const body = serialiseWorkflow(workflow);
      if (props.mode === "edit") {
        await update(body);
        router.push(`/workflows/${props.workflowId}`);
      } else {
        const created = await create(body);
        router.push(`/workflows/${created.id}`);
      }
    } catch (e) {
      setSubmissionError(e instanceof Error ? e.message : "Unknown error");
    }
  }, [create, router, update, workflow, props]);

  const canDelete = workflow.steps.length > 1 || (selectedPath?.split("/").length ?? 0) > 2;

  const heading = isEdit ? "Edit workflow" : "Build a workflow";
  const subheading = isEdit
    ? "Tweak steps and settings, then save your changes."
    : "Sketch the steps, click any node to edit, and POST to create it.";
  const submitLabel = isEdit
    ? busy
      ? "Saving…"
      : "Save Changes"
    : busy
      ? "Creating…"
      : "Create Workflow";

  return (
    <div className="flex flex-col h-[calc(100vh-var(--height-navbar))]">
      <div className="px-8 pt-6 pb-3 space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-extrabold tracking-tight text-fg flex items-center gap-2">
              <WorkflowIcon size={20} className="text-fg-subtle" />
              {heading}
            </h1>
            <p className="text-fg-muted text-sm mt-1">{subheading}</p>
          </div>
          <div className="flex items-center gap-2">
            <div className="relative">
              <Button
                intent="secondary"
                size="md"
                shape="pill"
                icon={<Plus size={14} />}
                onPress={() => setPickerOpen((v) => !v)}
              >
                Add step
              </Button>
              {pickerOpen && (
                <div className="absolute right-0 top-full mt-2 z-30">
                  <KindPicker
                    onPick={handleAddStep}
                    onClose={() => setPickerOpen(false)}
                  />
                </div>
              )}
            </div>
            <Button
              intent="primary"
              size="md"
              shape="pill"
              onPress={handleSubmit}
              isDisabled={busy || workflow.steps.length === 0 || !workflow.name.trim()}
            >
              {submitLabel}
            </Button>
          </div>
        </div>
        <WorkflowHeaderCard workflow={workflow} onChange={setWorkflow} />
        {(submissionError || serverError) && (
          <div className="bg-status-failed-bg border border-status-failed-border text-status-failed rounded-card px-4 py-2 text-sm">
            {submissionError ?? serverError}
          </div>
        )}
      </div>

      <div className="flex-1 flex border-t border-border-subtle min-h-0">
        <div className="flex-1 min-w-0 bg-surface-secondary relative">
          <ReactFlow
            nodes={nodes}
            edges={edges}
            nodeTypes={nodeTypes}
            onNodeClick={handleNodeClick}
            onPaneClick={() => setSelectedPath(null)}
            fitView
            fitViewOptions={{ padding: 0.2 }}
            proOptions={{ hideAttribution: true }}
            nodesDraggable
            nodesConnectable={false}
            elementsSelectable
          >
            <Background gap={20} color="var(--color-border)" />
            <MiniMap pannable zoomable maskColor="rgba(243,244,246,0.6)" />
            <Controls position="bottom-right" showInteractive={false} />
          </ReactFlow>
          {workflow.steps.length === 0 && (
            <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
              <div className="text-fg-muted text-sm">
                No steps yet — click &ldquo;Add step&rdquo; above.
              </div>
            </div>
          )}
        </div>
        {selectedStep && selectedPath && (
          <StepEditorPanel
            step={selectedStep}
            path={selectedPath}
            onChange={handleStepChange}
            onClose={() => setSelectedPath(null)}
            onDelete={handleStepDelete}
            canDelete={canDelete}
          />
        )}
      </div>
    </div>
  );
}

/**
 * Converts a server-shape `Job` (workflow read model) into the local
 * `NewWorkflow` shape used by the editor. Strips server-managed fields
 * (`id`, `version`, `created_at`, `updated_at`, `last_run_*`,
 * `next_run_at`, `is_favorited`) and normalises null-vs-undefined for
 * the optional fields. The step list is cast through `unknown` because
 * the read-side `WorkflowStep` union doesn't enumerate all kinds the
 * backend actually supports (script, match, set_var) — they round-trip
 * as opaque structured data.
 */
function jobToNewWorkflow(job: Job): NewWorkflow {
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

/**
 * Strips empty / undefined fields out of the NewWorkflow before sending
 * it to the backend. The backend's struct uses `#[serde(default)]` so
 * absent fields are fine; sending `null` for some fields would actually
 * conflict with the type. PATCH semantics: this whole payload replaces
 * the workflow's mutable fields on the server.
 */
function serialiseWorkflow(workflow: NewWorkflow): Record<string, unknown> {
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
