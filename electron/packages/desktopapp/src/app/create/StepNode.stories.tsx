/**
 * StepNode stories
 *
 * Renders the custom reactflow node in isolation against a stub
 * `ReactFlowProvider` so the `<Handle>` elements mount without the
 * full editor canvas. Each story exercises one of the visual states
 * the node can land in:
 *
 *   - Idle — default node, no hover, not selected.
 *   - Hovered — actions row + drag-grip become visible (the harness
 *     applies the `group` hover state by simulating a hover.)
 *   - Selected — reactflow's `selected` prop is true; brand ring shows.
 *   - Disconnected — dashed brand border + "not wired" badge.
 *   - HoverActionsVisible — combines hover state so the pencil/trash
 *     row is captured for visual regression.
 *
 * The stories don't need the full editor wiring (positions, drag,
 * reconcile) — they just confirm StepNode renders correctly in each
 * variant. Callbacks are no-ops; the modal-open behaviour lives in
 * `WorkflowGraphEditor`, not in this component.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { ReactFlow, ReactFlowProvider, type Node } from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import { StepNode } from "./StepNode";
import type { StepNodeData } from "./graph";
import type { NewStep } from "./types";
import { EDITOR_CURSOR_CSS } from "./cursors";

const SHELL_STEP: NewStep = {
  id: "say_hello",
  kind: "shell",
  command: "echo hello",
};

const nodeTypes = { step: StepNode };

function buildNode(
  overrides: Partial<StepNodeData> = {},
  selected = false,
): Node<StepNodeData> {
  return {
    id: "s/0",
    type: "step",
    position: { x: 0, y: 0 },
    selected,
    data: {
      step: SHELL_STEP,
      path: "s/0",
      summary: "echo hello",
      canDelete: true,
      onEdit: () => {},
      onDelete: () => {},
      onSwitchKind: () => {},
      onReorder: () => {},
      ...overrides,
    },
  };
}

function Frame({ node }: { node: Node<StepNodeData> }) {
  return (
    <div
      data-acs-editor
      className="w-[480px] h-[260px] bg-surface-tertiary border border-border rounded-card overflow-hidden"
    >
      <style>{EDITOR_CURSOR_CSS}</style>
      <ReactFlowProvider>
        <ReactFlow
          nodes={[node]}
          edges={[]}
          nodeTypes={nodeTypes}
          fitView
          fitViewOptions={{ padding: 0.6 }}
          proOptions={{ hideAttribution: true }}
          nodesDraggable={false}
          nodesConnectable
          panOnDrag={false}
          zoomOnScroll={false}
          zoomOnPinch={false}
        />
      </ReactFlowProvider>
    </div>
  );
}

const meta: Meta<typeof StepNode> = {
  title: "Pages/CreateWorkflow/StepNode",
  component: StepNode,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof StepNode>;

export const Idle: Story = {
  render: () => <Frame node={buildNode()} />,
};

export const Selected: Story = {
  render: () => <Frame node={buildNode({}, true)} />,
};

export const Disconnected: Story = {
  render: () => <Frame node={buildNode({ disconnected: true })} />,
};

/**
 * Force the `group:hover` state by adding a CSS rule that flips
 * `:hover`-targeted selectors on permanently for this story. This is
 * the only reliable way to capture the hover-actions row in
 * Storybook's static screenshot — we can't simulate a real
 * mouseenter on the reactflow surface.
 */
export const Hovered: Story = {
  render: () => (
    <>
      <style>{`
        .acs-force-hover .group:hover, .acs-force-hover .group { /* no-op anchor */ }
        .acs-force-hover [class*="group-hover"] { opacity: 1 !important; }
        .acs-force-hover .group .hidden { display: flex !important; }
      `}</style>
      <div className="acs-force-hover">
        <Frame node={buildNode()} />
      </div>
    </>
  ),
};

export const HoverActionsVisible: Story = {
  render: () => (
    <>
      <style>{`
        .acs-force-hover .group .hidden { display: flex !important; }
      `}</style>
      <div className="acs-force-hover">
        <Frame node={buildNode()} />
      </div>
    </>
  ),
};
