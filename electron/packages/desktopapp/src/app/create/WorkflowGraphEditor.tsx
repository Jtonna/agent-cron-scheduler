"use client";

/**
 * WorkflowGraphEditor
 *
 * The interactive editor at the heart of /create AND /workflows/[id]/edit.
 * Owns the `NewWorkflow` state and renders the new whiteboard-style
 * canvas:
 *
 *   - ScheduleCard (top-centre, compact floating card with cron + tz +
 *     enabled toggle + Save/Create primary button)
 *   - MiniMap (top-left, with a "Minimap" eyebrow label so users know
 *     what the aerial view is for)
 *   - Controls (zoom in/out/fit, top-right)
 *   - KindPaletteTray (left dock, vertically centred so it coexists with
 *     the minimap above; chip click appends a step at the end)
 *   - ReactFlow canvas with custom StepNode + InsertEdge (mid-edge +
 *     button that opens a kind picker), dot-grid background on a
 *     muted surface that gives the white cursor real contrast
 *   - StepEditorModal stack (palette-style modal; nested when drilling
 *     into match cases — each level pushes a frame onto the modalStack)
 *
 * The workflow name is NOT rendered here — it lives in the
 * `CanvasBreadcrumb` above the canvas, where the parent page wires it up
 * via the `onNameChange` prop the editor exposes for that purpose. This
 * keeps the whiteboard clean and the breadcrumb authoritative.
 *
 * The component runs in one of two modes via discriminated-union props:
 *   - `mode: "create"` — seeds with a fresh `NewWorkflow`, submits via
 *     `useCreateWorkflow`, navigates to the newly-created workflow's
 *     detail page on success.
 *   - `mode: "edit"` — seeds from an existing `Workflow` (the `Job` read
 *     shape from `apis/types.ts`) after stripping server-only fields,
 *     submits via `useUpdateWorkflow`, navigates back to the same
 *     workflow's detail page on success.
 */

import { useCallback, useMemo, useState } from "react";
import { useRouter } from "next/navigation";
import {
  Background,
  BackgroundVariant,
  Controls,
  MiniMap,
  ReactFlow,
  ReactFlowProvider,
  type EdgeTypes,
  type Node,
  type NodeMouseHandler,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";

import { useCreateWorkflow } from "@/apis/useCreateWorkflow";
import { useUpdateWorkflow } from "@/apis/useUpdateWorkflow";
import type { Job } from "@/apis/types";
import { StepNode } from "./StepNode";
import { StepEditorModal, type BreadcrumbCrumb } from "./StepEditorModal";
import { ScheduleCard } from "./ScheduleCard";
import { KindPaletteTray } from "./KindPaletteTray";
import { InsertEdge } from "./EdgePlusButton";
import {
  buildGraph,
  deleteStepAtPath,
  getStepAtPath,
  insertStepAfter,
  layoutGraph,
  reorderStepAtPath,
  updateStepAtPath,
  type StepNodeData,
} from "./graph";
import { makeDefaultStep } from "./types";
import type { NewStep, NewWorkflow, StepKind } from "./types";

const nodeTypes = { step: StepNode };
const edgeTypes: EdgeTypes = { insert: InsertEdge };

interface WorkflowGraphEditorPropsCommon {
  /**
   * Controlled workflow name. When provided, this value overrides the
   * editor's internal `workflow.name` on every render — the parent
   * (typically the page hosting the `CanvasBreadcrumb` editable crumb)
   * becomes the source of truth for the name. When omitted, the editor
   * keeps using whatever `initialWorkflow.name` it was seeded with.
   */
  name?: string;
  /**
   * Notifies the parent of name changes so it can keep the breadcrumb
   * (or any other out-of-canvas name editor) in sync.
   */
  onNameChange?: (name: string) => void;
}

export type WorkflowGraphEditorProps =
  | ({ mode: "create"; initialWorkflow: NewWorkflow } & WorkflowGraphEditorPropsCommon)
  | ({ mode: "edit"; workflowId: string; initialWorkflow: Job } & WorkflowGraphEditorPropsCommon);

/**
 * One frame on the modal stack — identifies which step the user is
 * currently editing by its path. The breadcrumb is derived from the
 * stack when rendering.
 */
interface ModalFrame {
  path: string;
}

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
    // Seed once — see WorkflowGraphEditor history for the rationale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [],
  );

  const [internalWorkflow, setWorkflow] = useState<NewWorkflow>(seed);
  const [modalStack, setModalStack] = useState<ModalFrame[]>([]);

  // Controlled-name overlay. If the parent passes `name`, that's the
  // source of truth — derive the effective `workflow` by overlaying it
  // on top of the internal state. No effect, no setState loop, no
  // custom setter wrapper: the existing `setWorkflow` calls below only
  // ever touch steps/schedule/etc., never `name`, so the overlay stays
  // authoritative for the displayed name. `onNameChange` is wired in
  // for symmetry / future use.
  const { name: controlledName } = props;
  const workflow: NewWorkflow = useMemo(
    () =>
      controlledName !== undefined
        ? { ...internalWorkflow, name: controlledName }
        : internalWorkflow,
    [controlledName, internalWorkflow],
  );

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

  /* ── Mutations ────────────────────────────────────────────────────── */

  const handleEdit = useCallback((path: string) => {
    setModalStack([{ path }]);
  }, []);

  const handleDelete = useCallback((path: string) => {
    setWorkflow((prev) => ({
      ...prev,
      steps: deleteStepAtPath(prev.steps, path),
    }));
    setModalStack((prev) => prev.filter((f) => f.path !== path));
  }, []);

  const handleReorder = useCallback((path: string, dir: "up" | "down") => {
    setWorkflow((prev) => ({
      ...prev,
      steps: reorderStepAtPath(prev.steps, path, dir),
    }));
  }, []);

  const handleSwitchKind = useCallback((path: string) => {
    // Same surface as Edit — open the modal; the kind switcher lives in its header.
    setModalStack([{ path }]);
  }, []);

  const handleInsertAfter = useCallback((sourcePath: string, kind: StepKind) => {
    const newStep = makeDefaultStep(kind);
    setWorkflow((prev) => ({
      ...prev,
      steps: insertStepAfter(prev.steps, sourcePath, newStep),
    }));
  }, []);

  const handleAppend = useCallback((kind: StepKind) => {
    const newStep = makeDefaultStep(kind);
    setWorkflow((prev) => ({
      ...prev,
      steps: insertStepAfter(prev.steps, null, newStep),
    }));
  }, []);

  /* ── Graph build ──────────────────────────────────────────────────── */

  const { nodes, edges } = useMemo(() => {
    const built = buildGraph(workflow.steps, {
      onEdit: handleEdit,
      onDelete: handleDelete,
      onSwitchKind: handleSwitchKind,
      onReorder: handleReorder,
      onInsertAfter: handleInsertAfter,
    });
    const positioned = layoutGraph(built.nodes, built.edges);
    return { nodes: positioned, edges: built.edges };
  }, [
    workflow.steps,
    handleEdit,
    handleDelete,
    handleSwitchKind,
    handleReorder,
    handleInsertAfter,
  ]);

  /* ── Click handlers ───────────────────────────────────────────────── */

  const handleNodeClick: NodeMouseHandler = useCallback((_event, node: Node) => {
    const data = node.data as StepNodeData;
    setModalStack([{ path: data.path }]);
  }, []);

  /* ── Modal stack helpers ──────────────────────────────────────────── */

  const currentFrame = modalStack[modalStack.length - 1];
  const currentStep = currentFrame ? getStepAtPath(workflow.steps, currentFrame.path) : null;

  const handleStepChange = useCallback(
    (next: NewStep) => {
      if (!currentFrame) return;
      setWorkflow((prev) => ({
        ...prev,
        steps: updateStepAtPath(prev.steps, currentFrame.path, next),
      }));
    },
    [currentFrame],
  );

  const handleOpenNested = useCallback(
    (caseKey: string | null) => {
      if (!currentFrame) return;
      const parentStep = getStepAtPath(workflow.steps, currentFrame.path);
      if (!parentStep || parentStep.kind !== "match") return;
      const childList =
        caseKey === null ? parentStep.default ?? [] : parentStep.cases[caseKey] ?? [];
      if (childList.length === 0) {
        // Drilling into an empty case — append a placeholder shell so the
        // user has something to edit. Otherwise getStepAtPath returns null.
        const placeholder = makeDefaultStep("shell");
        setWorkflow((prev) => ({
          ...prev,
          steps: updateStepAtPath(prev.steps, currentFrame.path, {
            ...parentStep,
            cases:
              caseKey !== null
                ? { ...parentStep.cases, [caseKey]: [placeholder] }
                : parentStep.cases,
            default: caseKey === null ? [placeholder] : parentStep.default,
          }),
        }));
      }
      const childPath =
        caseKey === null
          ? `${currentFrame.path}/default/0`
          : `${currentFrame.path}/cases/${caseKey}/0`;
      setModalStack((prev) => [...prev, { path: childPath }]);
    },
    [currentFrame, workflow.steps],
  );

  const popModal = useCallback(() => {
    setModalStack((prev) => prev.slice(0, -1));
  }, []);

  const closeAllModals = useCallback(() => {
    setModalStack([]);
  }, []);

  /* ── Submit ───────────────────────────────────────────────────────── */

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

  const submitLabel = isEdit
    ? busy
      ? "Saving…"
      : "Save Changes"
    : busy
      ? "Creating…"
      : "Create Workflow";

  /* ── Breadcrumb derivation for nested modals ──────────────────────── */

  const breadcrumb = useMemo<BreadcrumbCrumb[]>(() => {
    if (modalStack.length <= 1) return [];
    const crumbs: BreadcrumbCrumb[] = [];
    for (let i = 0; i < modalStack.length - 1; i++) {
      const frame = modalStack[i];
      const step = getStepAtPath(workflow.steps, frame.path);
      const label = step?.id ?? frame.path.split("/").pop() ?? "step";
      const targetIndex = i;
      crumbs.push({
        label,
        onClick: () => setModalStack((prev) => prev.slice(0, targetIndex + 1)),
      });
    }
    return crumbs;
  }, [modalStack, workflow.steps]);

  return (
    <div className="flex flex-col h-[calc(100vh-var(--height-navbar))]">
      <div className="flex-1 relative bg-surface-tertiary overflow-hidden">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          edgeTypes={edgeTypes}
          onNodeClick={handleNodeClick}
          fitView
          fitViewOptions={{ padding: 0.25 }}
          proOptions={{ hideAttribution: true }}
          nodesDraggable
          nodesConnectable={false}
          elementsSelectable
        >
          {/* Dot-grid canvas background — makes the whiteboard feel like
              a real surface and gives the white cursor something to land
              against. Color uses an existing faint foreground token. */}
          <Background
            variant={BackgroundVariant.Dots}
            gap={20}
            size={1.25}
            color="var(--color-fg-faint)"
          />
          <Controls
            position="top-right"
            showInteractive={false}
            className="!bg-surface !border !border-border !rounded-card !shadow-sm"
          />
        </ReactFlow>

        {/* ── Top-left: labeled minimap stack ───────────────────────── */}
        <div className="absolute top-4 left-4 z-20 flex flex-col gap-1">
          <span className="text-eyebrow pl-0.5">Minimap</span>
          <MiniMap
            pannable
            zoomable
            position="top-left"
            maskColor="rgba(243,244,246,0.6)"
            className="!relative !top-0 !left-0 !m-0 !bg-surface !border !border-border !rounded-card !shadow-sm"
          />
        </div>

        {/* ── Top-centre: schedule card + primary action ─────────────── */}
        <div className="absolute top-4 left-1/2 -translate-x-1/2 z-20">
          <ScheduleCard
            workflow={workflow}
            onChange={setWorkflow}
            submitLabel={submitLabel}
            onSubmit={handleSubmit}
            submitDisabled={busy || workflow.steps.length === 0 || !workflow.name.trim()}
          />
        </div>

        {/* ── Left dock: kind palette tray (vertically centred so it
             coexists with the minimap stack above) ─────────────────── */}
        <KindPaletteTray onAdd={handleAppend} />

        {/* ── Empty state hint ───────────────────────────────────────── */}
        {workflow.steps.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <div className="text-fg-muted text-sm">
              No steps yet — pick a kind from the left palette.
            </div>
          </div>
        )}

        {/* ── Submission / server errors ─────────────────────────────── */}
        {(submissionError || serverError) && (
          <div className="absolute bottom-4 left-1/2 -translate-x-1/2 z-20 max-w-md bg-status-failed-bg border border-status-failed-border text-status-failed rounded-card px-4 py-2 text-sm shadow-menu">
            {submissionError ?? serverError}
          </div>
        )}
      </div>

      {/* ── Modal stack (palette-style step editor + nested frames) ─── */}
      {currentStep && currentFrame && (
        <StepEditorModal
          key={currentFrame.path}
          step={currentStep}
          breadcrumb={breadcrumb}
          onChange={handleStepChange}
          onClose={() => (modalStack.length > 1 ? popModal() : closeAllModals())}
          onOpenNested={handleOpenNested}
        />
      )}
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
