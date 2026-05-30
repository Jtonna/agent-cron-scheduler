/**
 * graph
 *
 * Derives a reactflow node + edge set from a `NewWorkflow.steps[]` array
 * and runs dagre for left-to-right auto-layout. Match steps fan out into
 * their cases (and default), each rendered as its own sub-chain that
 * rejoins after the match.
 *
 * Node ids are paths that uniquely identify the step's location in the
 * tree (e.g. `s/0` for top-level step 0, `s/2/cases/ok/1` for the second
 * step inside the `ok` case of top-level step 2). The pure mutation
 * helpers that operate on those same paths (get/update/delete/reorder/
 * insertAfter) live in `graphPaths.ts` and are re-exported from this
 * module for convenience so existing call sites can keep importing
 * everything graph-related from one place.
 */

import dagre from "dagre";
import type { Edge, Node } from "@xyflow/react";
import type { NewStep, StepKind } from "./types";
import { summarizeStep } from "./stepMeta";

export {
  getStepAtPath,
  updateStepAtPath,
  deleteStepAtPath,
  reorderStepAtPath,
  insertStepAfter,
  reorderViaEdgeReconnect,
  getScopeForPath,
  sameScope,
  type ChainScope,
} from "./graphPaths";

export interface StepNodeData extends Record<string, unknown> {
  step: NewStep;
  path: string;
  summary: string;
  /** True if this is a `match` sub-chain "ghost" terminator. */
  isJoin?: boolean;
  /** Label for a branch (case key / "default"), if any. */
  branchLabel?: string;
  /** Whether this node may be deleted (the editor disables delete for the last top-level step). */
  canDelete?: boolean;
  /* ── Imperative callbacks wired by the parent editor. ───────────── */
  onEdit?: (path: string) => void;
  onDelete?: (path: string) => void;
  onSwitchKind?: (path: string) => void;
  onReorder?: (path: string, dir: "up" | "down") => void;
}

const NODE_WIDTH = 240;
const NODE_HEIGHT = 92;

interface BuildResult {
  nodes: Node<StepNodeData>[];
  edges: Edge[];
}

export interface BuildOptions {
  onEdit?: (path: string) => void;
  onDelete?: (path: string) => void;
  onSwitchKind?: (path: string) => void;
  onReorder?: (path: string, dir: "up" | "down") => void;
  /** Called by the InsertEdge custom edge when the user picks a kind to insert. */
  onInsertAfter?: (sourcePath: string, kind: StepKind) => void;
  /** Whether the editor permits deleting the final remaining top-level step. */
  topLevelCanDelete?: boolean;
}

/**
 * Walks the step tree depth-first and returns a flat list of reactflow
 * nodes + edges. The returned graph is not yet positioned — call
 * `layoutGraph` to run dagre.
 */
export function buildGraph(steps: NewStep[], opts: BuildOptions = {}): BuildResult {
  const nodes: Node<StepNodeData>[] = [];
  const edges: Edge[] = [];

  function addStep(
    step: NewStep,
    path: string,
    branchLabel?: string,
    canDelete = true,
  ): void {
    nodes.push({
      id: path,
      type: "step",
      data: {
        step,
        path,
        summary: summarizeStep(step),
        branchLabel,
        canDelete,
        onEdit: opts.onEdit,
        onDelete: opts.onDelete,
        onSwitchKind: opts.onSwitchKind,
        onReorder: opts.onReorder,
      },
      position: { x: 0, y: 0 },
    });
  }

  /**
   * Renders a linear chain of steps and returns the entry / exit node
   * ids of the chain (or null if the chain is empty).
   */
  function renderChain(
    chainSteps: NewStep[],
    basePath: string,
    branchLabel?: string,
  ): { entry: string | null; exit: string | null } {
    if (chainSteps.length === 0) return { entry: null, exit: null };

    let entry: string | null = null;
    let prevExit: string | null = null;

    for (let i = 0; i < chainSteps.length; i++) {
      const step = chainSteps[i];
      const path = `${basePath}/${i}`;
      const isTopLevel = basePath === "s";
      const canDelete = !isTopLevel || chainSteps.length > 1 || (opts.topLevelCanDelete ?? false);
      addStep(step, path, i === 0 ? branchLabel : undefined, canDelete);

      if (i === 0) entry = path;

      if (prevExit) {
        edges.push({
          id: `e:${prevExit}->${path}`,
          source: prevExit,
          target: path,
          type: opts.onInsertAfter ? "insert" : "smoothstep",
          // `reconnectable: "target"` makes only the head end draggable.
          // The semantic is "after node A, the next node is now C".
          reconnectable: "target",
          data: opts.onInsertAfter
            ? { sourcePath: prevExit, onInsert: opts.onInsertAfter }
            : undefined,
        });
      }

      // If this is a match step, render its branches and let them
      // become the "exit" of this step (each branch chain becomes a
      // separate continuation; we pick the last branch's exit as the
      // canonical exit for chain-rejoin purposes — this is a prototype,
      // so real rejoin semantics are best-effort).
      if (step.kind === "match") {
        const caseEntries = Object.entries(step.cases ?? {});
        const branchExits: string[] = [];

        for (const [caseKey, caseSteps] of caseEntries) {
          const { entry: caseEntry, exit: caseExit } = renderChain(
            caseSteps,
            `${path}/cases/${caseKey}`,
            caseKey,
          );
          if (caseEntry) {
            edges.push({
              id: `e:${path}->${caseEntry}`,
              source: path,
              target: caseEntry,
              type: "smoothstep",
              label: caseKey,
              style: { stroke: "var(--color-fg-subtle)" },
              labelStyle: { fontFamily: "monospace", fontSize: 10 },
            });
          }
          if (caseExit) branchExits.push(caseExit);
        }

        if (step.default && step.default.length > 0) {
          const { entry: defEntry, exit: defExit } = renderChain(
            step.default,
            `${path}/default`,
            "default",
          );
          if (defEntry) {
            edges.push({
              id: `e:${path}->${defEntry}`,
              source: path,
              target: defEntry,
              type: "smoothstep",
              label: "default",
              style: { stroke: "var(--color-fg-subtle)" },
            });
          }
          if (defExit) branchExits.push(defExit);
        }

        // For prototype purposes, the chain "exit" after a match is the
        // match node itself if no branches existed, otherwise the last
        // branch's tail.
        prevExit = branchExits.length > 0 ? branchExits[branchExits.length - 1] : path;
      } else {
        prevExit = path;
      }
    }

    return { entry, exit: prevExit };
  }

  renderChain(steps, "s");

  return { nodes, edges };
}

/**
 * Runs dagre over the built graph and writes back positions in place.
 * Left-to-right layout (rankdir LR) reads like a flowchart.
 */
export function layoutGraph(nodes: Node<StepNodeData>[], edges: Edge[]): Node<StepNodeData>[] {
  const g = new dagre.graphlib.Graph();
  g.setGraph({ rankdir: "LR", nodesep: 36, ranksep: 80, marginx: 24, marginy: 24 });
  g.setDefaultEdgeLabel(() => ({}));

  for (const node of nodes) {
    g.setNode(node.id, { width: NODE_WIDTH, height: NODE_HEIGHT });
  }
  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }

  dagre.layout(g);

  return nodes.map((node) => {
    const pos = g.node(node.id);
    return {
      ...node,
      position: { x: pos.x - NODE_WIDTH / 2, y: pos.y - NODE_HEIGHT / 2 },
      width: NODE_WIDTH,
      height: NODE_HEIGHT,
    };
  });
}
