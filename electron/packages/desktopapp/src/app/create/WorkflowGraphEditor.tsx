"use client";

/**
 * WorkflowGraphEditor
 *
 * The interactive canvas at the heart of /create AND /workflows/[id]/edit.
 * Owns the `NewWorkflow.steps` mutation surface and renders the
 * whiteboard-style canvas:
 *
 *   - MiniMap (top-left, with a "Minimap" eyebrow label so users know
 *     what the aerial view is for)
 *   - Controls (zoom in/out/fit, top-right)
 *   - KindPaletteDock (bottom-centre horizontal dock; chip click appends
 *     a step at the end)
 *   - ReactFlow canvas with custom StepNode + InsertEdge (mid-edge +
 *     button that opens a kind picker), dot-grid background on a
 *     muted surface that gives the white cursor real contrast
 *   - StepEditorModal stack (palette-style modal; nested when drilling
 *     into match cases — each level pushes a frame onto the modal
 *     stack, owned by `useStepEditorStack`)
 *
 * The workflow identity (name, schedule, timezone, enabled) and the
 * Save / Create button NO LONGER live on the canvas. They live in the
 * `EditorSidebar` mounted by the page on the left. The editor exposes
 * controlled props so the page can keep both surfaces in sync without
 * the editor having to know about the sidebar.
 *
 * State / logic split:
 *   - The `NewWorkflow` state and its step-mutation callbacks (insert,
 *     delete, reorder, append) live in this file because they're tied
 *     to the canvas action surface.
 *   - The modal stack — drilling into match cases, breadcrumb
 *     derivation, live edit committer — lives in
 *     `useStepEditorStack`.
 *   - Server-shape `Job` ↔ `NewWorkflow` conversion lives in
 *     `workflowSerialization.ts`.
 *   - Submit wiring (create vs update, navigation, error state) lives
 *     in the page; the page reads `serialiseWorkflow(getWorkflow())`
 *     when the sidebar's submit button fires via the
 *     `onWorkflowChange` snapshot.
 *
 * The component runs in one of two modes via discriminated-union props:
 *   - `mode: "create"` — seeds with a fresh `NewWorkflow`.
 *   - `mode: "edit"` — seeds from an existing `Workflow` (the `Job` read
 *     shape from `apis/types.ts`) after stripping server-only fields.
 */

import { useCallback, useEffect, useMemo, useState } from "react";
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

import type { Job } from "@/apis/types";
import { StepNode } from "./StepNode";
import { StepEditorModal } from "./StepEditorModal";
import { KindPaletteDock } from "./KindPaletteDock";
import { InsertEdge } from "./EdgePlusButton";
import { EDITOR_CURSOR_CSS } from "./cursors";
import {
  buildGraph,
  deleteStepAtPath,
  insertStepAfter,
  layoutGraph,
  reorderStepAtPath,
  type StepNodeData,
} from "./graph";
import { makeDefaultStep } from "./types";
import type { NewWorkflow, StepKind } from "./types";
import { useStepEditorStack } from "./useStepEditorStack";
import { jobToNewWorkflow } from "./workflowSerialization";

const nodeTypes = { step: StepNode };
const edgeTypes: EdgeTypes = { insert: InsertEdge };

interface WorkflowGraphEditorPropsCommon {
  /**
   * Controlled workflow name. Sourced from the page-level state shared
   * with `EditorSidebar`. Overrides whatever name the editor was
   * seeded with on every render.
   */
  name: string;
  /**
   * Controlled schedule. Sourced from the page-level state shared with
   * `EditorSidebar`.
   */
  schedule: string;
  /** Controlled timezone. */
  timezone: string;
  /** Controlled enabled flag. */
  enabled: boolean;
  /**
   * Fires whenever the editor mutates the workflow (step insert /
   * delete / reorder / nested change inside the modal). The page uses
   * this to keep its `NewWorkflow` snapshot in sync so the sidebar's
   * Save button can serialise the latest state.
   */
  onWorkflowChange?: (next: NewWorkflow) => void;
}

export type WorkflowGraphEditorProps =
  | ({ mode: "create"; initialWorkflow: NewWorkflow } & WorkflowGraphEditorPropsCommon)
  | ({
      mode: "edit";
      workflowId: string;
      initialWorkflow: Job;
    } & WorkflowGraphEditorPropsCommon);

export function WorkflowGraphEditor(props: WorkflowGraphEditorProps) {
  return (
    <ReactFlowProvider>
      <WorkflowGraphEditorInner {...props} />
    </ReactFlowProvider>
  );
}

function WorkflowGraphEditorInner(props: WorkflowGraphEditorProps) {
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

  // Controlled overlays. The page owns name/schedule/timezone/enabled;
  // the editor's internal `setWorkflow` calls below only ever touch
  // `steps`, so the overlay stays authoritative for the controlled
  // fields without an effect or setState loop.
  const { name, schedule, timezone, enabled, onWorkflowChange } = props;
  const workflow: NewWorkflow = useMemo(
    () => ({
      ...internalWorkflow,
      name,
      schedule,
      timezone: timezone.length > 0 ? timezone : undefined,
      enabled,
    }),
    [internalWorkflow, name, schedule, timezone, enabled],
  );

  // Tell the page about the latest snapshot whenever it changes so the
  // submit handler always has fresh `steps`.
  useEffect(() => {
    onWorkflowChange?.(workflow);
  }, [workflow, onWorkflowChange]);

  const stack = useStepEditorStack(workflow, setWorkflow);

  /* ── Mutations ────────────────────────────────────────────────────── */

  const handleEdit = useCallback(
    (path: string) => stack.openAt(path),
    [stack],
  );

  const handleDelete = useCallback(
    (path: string) => {
      setWorkflow((prev) => ({
        ...prev,
        steps: deleteStepAtPath(prev.steps, path),
      }));
      stack.forgetPath(path);
    },
    [stack],
  );

  const handleReorder = useCallback((path: string, dir: "up" | "down") => {
    setWorkflow((prev) => ({
      ...prev,
      steps: reorderStepAtPath(prev.steps, path, dir),
    }));
  }, []);

  const handleSwitchKind = useCallback(
    // Same surface as Edit — open the modal; the kind switcher lives in its header.
    (path: string) => stack.openAt(path),
    [stack],
  );

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

  const handleNodeClick: NodeMouseHandler = useCallback(
    (_event, node: Node) => {
      const data = node.data as StepNodeData;
      stack.openAt(data.path);
    },
    [stack],
  );

  const { currentStep, currentFrame, breadcrumb, handleStepChange, handleOpenNested, pop, closeAll } =
    stack;

  return (
    <div
      data-acs-editor
      className="flex-1 flex flex-col min-h-0"
    >
      {/* Accessibility: force high-contrast SVG cursors throughout the
          editor surface so OSX-style white cursors stay visible against
          the near-white canvas. Scoped to [data-acs-editor]. */}
      <style>{EDITOR_CURSOR_CSS}</style>
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

        {/* ── Bottom-centre dock: kind palette ──────────────────────── */}
        <KindPaletteDock onAdd={handleAppend} />

        {/* ── Empty state hint ───────────────────────────────────────── */}
        {workflow.steps.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
            <div className="text-fg-muted text-sm">
              No steps yet — pick a kind from the dock below.
            </div>
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
          onClose={() => (breadcrumb.length > 0 ? pop() : closeAll())}
          onOpenNested={handleOpenNested}
        />
      )}
    </div>
  );
}
