"use client";

/**
 * WorkflowGraphEditor
 *
 * The interactive editor at the heart of /create. Owns the
 * `NewWorkflow` state, renders:
 *   - the WorkflowHeaderCard (name / schedule / timezone / enabled)
 *   - a reactflow canvas with dagre-laid-out step nodes
 *   - a slide-in StepEditorPanel when a node is selected
 *   - a toolbar with "Add step" (via KindPicker) + "Create Workflow"
 *
 * The graph is derived from `state.steps` every render; updates flow
 * through `getStepAtPath` / `updateStepAtPath` / `deleteStepAtPath` /
 * `insertStepAfter` helpers in `graph.ts`. Submission posts to
 * `POST /api/workflows` via `useCreateWorkflow` and navigates to the
 * new workflow's detail page on success.
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

interface WorkflowGraphEditorProps {
  initialWorkflow: NewWorkflow;
}

export function WorkflowGraphEditor({ initialWorkflow }: WorkflowGraphEditorProps) {
  return (
    <ReactFlowProvider>
      <WorkflowGraphEditorInner initialWorkflow={initialWorkflow} />
    </ReactFlowProvider>
  );
}

function WorkflowGraphEditorInner({ initialWorkflow }: WorkflowGraphEditorProps) {
  const router = useRouter();
  const [workflow, setWorkflow] = useState<NewWorkflow>(initialWorkflow);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [pickerOpen, setPickerOpen] = useState(false);
  const { create, creating, error } = useCreateWorkflow();
  const [submissionError, setSubmissionError] = useState<string | null>(null);

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
      const created = await create(body);
      router.push(`/workflows/${created.id}`);
    } catch (e) {
      setSubmissionError(e instanceof Error ? e.message : "Unknown error");
    }
  }, [create, router, workflow]);

  const canDelete = workflow.steps.length > 1 || (selectedPath?.split("/").length ?? 0) > 2;

  return (
    <div className="flex flex-col h-[calc(100vh-var(--height-navbar))]">
      <div className="px-8 pt-6 pb-3 space-y-4">
        <div className="flex items-center justify-between">
          <div>
            <h1 className="text-2xl font-extrabold tracking-tight text-fg flex items-center gap-2">
              <WorkflowIcon size={20} className="text-fg-subtle" />
              Build a workflow
            </h1>
            <p className="text-fg-muted text-sm mt-1">
              Sketch the steps, click any node to edit, and POST to create it.
            </p>
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
              isDisabled={creating || workflow.steps.length === 0 || !workflow.name.trim()}
            >
              {creating ? "Creating…" : "Create Workflow"}
            </Button>
          </div>
        </div>
        <WorkflowHeaderCard workflow={workflow} onChange={setWorkflow} />
        {(submissionError || error) && (
          <div className="bg-status-failed-bg border border-status-failed-border text-status-failed rounded-card px-4 py-2 text-sm">
            {submissionError ?? error}
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
 * Strips empty / undefined fields out of the NewWorkflow before sending
 * it to the backend. The backend's struct uses `#[serde(default)]` so
 * absent fields are fine; sending `null` for some fields would actually
 * conflict with the type.
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
