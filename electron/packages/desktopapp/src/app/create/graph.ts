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
 * step inside the `ok` case of top-level step 2). These paths are also
 * used by the side-panel editor to update the right step.
 */

import dagre from "dagre";
import type { Edge, Node } from "@xyflow/react";
import type { NewStep } from "./types";
import { summarizeStep } from "./stepMeta";

export interface StepNodeData extends Record<string, unknown> {
  step: NewStep;
  path: string;
  summary: string;
  /** True if this is a `match` sub-chain "ghost" terminator. */
  isJoin?: boolean;
  /** Label for a branch (case key / "default"), if any. */
  branchLabel?: string;
}

const NODE_WIDTH = 240;
const NODE_HEIGHT = 92;

interface BuildResult {
  nodes: Node<StepNodeData>[];
  edges: Edge[];
}

/**
 * Walks the step tree depth-first and returns a flat list of reactflow
 * nodes + edges. The returned graph is not yet positioned — call
 * `layoutGraph` to run dagre.
 */
export function buildGraph(steps: NewStep[]): BuildResult {
  const nodes: Node<StepNodeData>[] = [];
  const edges: Edge[] = [];

  function addStep(step: NewStep, path: string, branchLabel?: string): void {
    nodes.push({
      id: path,
      type: "step",
      data: {
        step,
        path,
        summary: summarizeStep(step),
        branchLabel,
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
      addStep(step, path, i === 0 ? branchLabel : undefined);

      if (i === 0) entry = path;

      if (prevExit) {
        edges.push({
          id: `e:${prevExit}->${path}`,
          source: prevExit,
          target: path,
          type: "smoothstep",
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

/* ── Path helpers for the side-panel editor ── */

/**
 * Resolves a step path to the step object. Returns null for invalid paths.
 */
export function getStepAtPath(steps: NewStep[], path: string): NewStep | null {
  const parts = path.split("/").slice(1); // drop leading "s"
  let current: NewStep[] = steps;
  let step: NewStep | null = null;

  for (let i = 0; i < parts.length; i++) {
    const part = parts[i];
    if (part === "cases") {
      const caseKey = parts[++i];
      if (step?.kind !== "match") return null;
      current = step.cases[caseKey] ?? [];
      continue;
    }
    if (part === "default") {
      if (step?.kind !== "match") return null;
      current = step.default ?? [];
      continue;
    }
    const idx = parseInt(part, 10);
    if (Number.isNaN(idx) || idx < 0 || idx >= current.length) return null;
    step = current[idx];
  }
  return step;
}

/**
 * Returns a new steps array with the step at `path` replaced by
 * `nextStep`. The original array is not mutated. Returns the input
 * unchanged on invalid paths.
 */
export function updateStepAtPath(
  steps: NewStep[],
  path: string,
  nextStep: NewStep,
): NewStep[] {
  const parts = path.split("/").slice(1);
  if (parts.length === 0) return steps;

  function recurse(list: NewStep[], remaining: string[]): NewStep[] {
    const head = remaining[0];
    const idx = parseInt(head, 10);
    if (Number.isNaN(idx) || idx < 0 || idx >= list.length) return list;

    const current = list[idx];

    if (remaining.length === 1) {
      const copy = [...list];
      copy[idx] = nextStep;
      return copy;
    }

    // remaining = [idx, "cases", key, rest...] or [idx, "default", rest...]
    if (current.kind !== "match") return list;
    const tag = remaining[1];
    if (tag === "cases") {
      const caseKey = remaining[2];
      const rest = remaining.slice(3);
      const updated: NewStep = {
        ...current,
        cases: {
          ...current.cases,
          [caseKey]: recurse(current.cases[caseKey] ?? [], rest),
        },
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    if (tag === "default") {
      const rest = remaining.slice(2);
      const updated: NewStep = {
        ...current,
        default: recurse(current.default ?? [], rest),
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    return list;
  }

  return recurse(steps, parts);
}

/**
 * Removes the step at `path`. Returns the input unchanged on invalid paths.
 */
export function deleteStepAtPath(steps: NewStep[], path: string): NewStep[] {
  const parts = path.split("/").slice(1);
  if (parts.length === 0) return steps;

  function recurse(list: NewStep[], remaining: string[]): NewStep[] {
    const head = remaining[0];
    const idx = parseInt(head, 10);
    if (Number.isNaN(idx) || idx < 0 || idx >= list.length) return list;

    if (remaining.length === 1) {
      return list.filter((_, i) => i !== idx);
    }

    const current = list[idx];
    if (current.kind !== "match") return list;
    const tag = remaining[1];
    if (tag === "cases") {
      const caseKey = remaining[2];
      const rest = remaining.slice(3);
      const updated: NewStep = {
        ...current,
        cases: {
          ...current.cases,
          [caseKey]: recurse(current.cases[caseKey] ?? [], rest),
        },
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    if (tag === "default") {
      const rest = remaining.slice(2);
      const updated: NewStep = {
        ...current,
        default: recurse(current.default ?? [], rest),
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    return list;
  }

  return recurse(steps, parts);
}

/**
 * Appends a step after the given path (sibling insertion). If `path`
 * is null, appends to the top-level chain.
 */
export function insertStepAfter(
  steps: NewStep[],
  path: string | null,
  newStep: NewStep,
): NewStep[] {
  if (path === null) {
    return [...steps, newStep];
  }
  const parts = path.split("/").slice(1);

  function recurse(list: NewStep[], remaining: string[]): NewStep[] {
    const head = remaining[0];
    const idx = parseInt(head, 10);
    if (Number.isNaN(idx)) return list;

    if (remaining.length === 1) {
      const copy = [...list];
      copy.splice(idx + 1, 0, newStep);
      return copy;
    }

    const current = list[idx];
    if (current.kind !== "match") return list;
    const tag = remaining[1];
    if (tag === "cases") {
      const caseKey = remaining[2];
      const rest = remaining.slice(3);
      const updated: NewStep = {
        ...current,
        cases: {
          ...current.cases,
          [caseKey]: recurse(current.cases[caseKey] ?? [], rest),
        },
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    if (tag === "default") {
      const rest = remaining.slice(2);
      const updated: NewStep = {
        ...current,
        default: recurse(current.default ?? [], rest),
      };
      const copy = [...list];
      copy[idx] = updated;
      return copy;
    }
    return list;
  }

  return recurse(steps, parts);
}
