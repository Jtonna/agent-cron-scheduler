/**
 * graphPaths
 *
 * Pure tree-mutation helpers for the workflow `steps[]` array, keyed by
 * the same path strings that `buildGraph` (see `graph.ts`) assigns to
 * each reactflow node — e.g. `s/0`, `s/2/cases/ok/1`.
 *
 * These helpers are deliberately separate from `graph.ts` so the
 * graph-building / layout code can stay focused on rendering, while the
 * editor's modal stack and node action callbacks can import only the
 * narrow mutation surface they need:
 *
 *   - `getStepAtPath`             — resolve a path to its step (or `null`)
 *   - `updateStepAtPath`          — replace a step in-place
 *   - `deleteStepAtPath`          — remove a step from its sibling array
 *   - `reorderStepAtPath`         — swap a step up/down within its siblings
 *   - `insertStepAfter`           — sibling insertion after a path (or at
 *                                   the top level when path is `null`)
 *   - `reorderViaEdgeReconnect`   — reorder the underlying linear chain
 *                                   to express "after node A, the next
 *                                   node is now node C" — used by the
 *                                   reactflow edge-reconnect handler
 *                                   (see WorkflowGraphEditor).
 *   - `getScopeForPath`           — derive the chain scope (top-level
 *                                   chain or match-case branch) that a
 *                                   given path belongs to.
 *   - `sameScope`                 — equality on `ChainScope` values, used
 *                                   to reject cross-branch reconnects.
 *   - `getChainForScope` /
 *     `setChainForScope`          — chain accessors for a given
 *                                   `ChainScope` (top-level / case /
 *                                   default). Exposed for scope-aware
 *                                   editor affordances (tail-+ button,
 *                                   scope-region overlays).
 *   - `appendStepToScope`         — appends a new step to the tail of the
 *                                   chain at a given scope and returns
 *                                   both the next workflow and the new
 *                                   step's path so callers can wire it
 *                                   up. Used by the scope-aware dock-
 *                                   drop and the per-chain tail-+ button.
 *
 * All helpers are pure, return new arrays on success, and return the
 * input unchanged on invalid paths.
 */

import type { NewStep, NewWorkflow } from "./types";

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
 * Moves the step at `path` one position up or down within its sibling
 * array. Returns the input unchanged if the move would go out of bounds
 * or the path is invalid.
 */
export function reorderStepAtPath(
  steps: NewStep[],
  path: string,
  dir: "up" | "down",
): NewStep[] {
  const parts = path.split("/").slice(1);
  if (parts.length === 0) return steps;

  function recurse(list: NewStep[], remaining: string[]): NewStep[] {
    const head = remaining[0];
    const idx = parseInt(head, 10);
    if (Number.isNaN(idx) || idx < 0 || idx >= list.length) return list;

    if (remaining.length === 1) {
      const target = dir === "up" ? idx - 1 : idx + 1;
      if (target < 0 || target >= list.length) return list;
      const copy = [...list];
      [copy[idx], copy[target]] = [copy[target], copy[idx]];
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

/* ── Edge reconnection ─────────────────────────────────────────────── */

/**
 * Identifies which linear chain a given step path belongs to. Reorders
 * triggered by edge reconnects are only meaningful within a single
 * scope — crossing match-case boundaries has no clean linear-chain
 * semantic.
 */
export type ChainScope =
  | { kind: "top-level" }
  | { kind: "case"; matchPath: string; caseKey: string }
  | { kind: "default"; matchPath: string };

/**
 * Given a node path like `s/2/cases/ok/1`, returns the scope of the
 * chain that contains the step (its parent chain). Returns `null` if
 * the path is malformed.
 */
export function getScopeForPath(path: string): ChainScope | null {
  const parts = path.split("/");
  if (parts.length < 2 || parts[0] !== "s") return null;
  if (Number.isNaN(parseInt(parts[parts.length - 1], 10))) return null;
  const parent = parts.slice(0, -1);
  if (parent.length === 1) return { kind: "top-level" };
  const lastTag = parent[parent.length - 2];
  if (lastTag === "cases") {
    const caseKey = parent[parent.length - 1];
    const matchPath = parent.slice(0, -2).join("/");
    return { kind: "case", matchPath, caseKey };
  }
  if (parent[parent.length - 1] === "default") {
    const matchPath = parent.slice(0, -1).join("/");
    return { kind: "default", matchPath };
  }
  return null;
}

/** Structural equality for `ChainScope` values. */
export function sameScope(a: ChainScope, b: ChainScope): boolean {
  if (a.kind !== b.kind) return false;
  if (a.kind === "top-level") return true;
  if (a.kind === "case" && b.kind === "case") {
    return a.matchPath === b.matchPath && a.caseKey === b.caseKey;
  }
  if (a.kind === "default" && b.kind === "default") {
    return a.matchPath === b.matchPath;
  }
  return false;
}

/**
 * Returns the chain (linear `NewStep[]`) for the given scope, or `null`
 * when the scope refers to a non-existent location (e.g. a case key
 * that's not present on the target match step, or a match path that
 * doesn't resolve to a `match` step).
 *
 * Exported because the editor needs to walk every scope's chain to (a)
 * find each scope's tail node for the tail-+ affordance and (b)
 * compute scope-region bounding boxes for the drop-zone highlight
 * overlay.
 */
export function getChainForScope(workflow: NewWorkflow, scope: ChainScope): NewStep[] | null {
  if (scope.kind === "top-level") return workflow.steps;
  const matchStep = getStepAtPath(workflow.steps, scope.matchPath);
  if (!matchStep || matchStep.kind !== "match") return null;
  if (scope.kind === "case") {
    return matchStep.cases[scope.caseKey] ?? null;
  }
  return matchStep.default ?? null;
}

/**
 * Returns a new workflow with the chain at the given scope replaced by
 * `nextChain`. Returns the input workflow unchanged when the scope
 * doesn't resolve (e.g. matchPath doesn't point at a `match` step).
 *
 * Exported alongside `getChainForScope` for the same reason — the
 * scope-aware editor affordances need a single mutation surface for
 * appending into any chain.
 */
export function setChainForScope(
  workflow: NewWorkflow,
  scope: ChainScope,
  nextChain: NewStep[],
): NewWorkflow {
  if (scope.kind === "top-level") {
    return { ...workflow, steps: nextChain };
  }
  const matchStep = getStepAtPath(workflow.steps, scope.matchPath);
  if (!matchStep || matchStep.kind !== "match") return workflow;
  let nextMatch: NewStep;
  if (scope.kind === "case") {
    nextMatch = {
      ...matchStep,
      cases: { ...matchStep.cases, [scope.caseKey]: nextChain },
    };
  } else {
    nextMatch = { ...matchStep, default: nextChain };
  }
  return {
    ...workflow,
    steps: updateStepAtPath(workflow.steps, scope.matchPath, nextMatch),
  };
}

function localIndex(path: string): number {
  const parts = path.split("/");
  const idx = parseInt(parts[parts.length - 1], 10);
  return Number.isNaN(idx) ? -1 : idx;
}

/**
 * Reorders the linear chain that contains both `sourceNodeId` (node A)
 * and `newTargetNodeId` (node C) so that C immediately follows A.
 *
 * Algorithm: find A and C's indices in their shared chain, splice C
 * out, then re-insert it at A's index + 1 (adjusting for the shift if
 * C originally sat before A).
 *
 * Returns the input workflow unchanged when:
 *   - either path doesn't resolve to a step in the supplied scope
 *   - the scope is malformed or doesn't exist
 *   - the drop is a no-op (`C === A` or C is already A's immediate next)
 *   - either path escapes the supplied scope (lives in a sub-chain)
 *
 * Cross-scope reconnects must be rejected by the caller via
 * `getScopeForPath` + `sameScope` before invoking this helper.
 */
export function reorderViaEdgeReconnect(
  workflow: NewWorkflow,
  sourceNodeId: string,
  newTargetNodeId: string,
  scope: ChainScope,
): NewWorkflow {
  if (sourceNodeId === newTargetNodeId) return workflow;

  const chain = getChainForScope(workflow, scope);
  if (!chain) return workflow;

  const expectedParent =
    scope.kind === "top-level"
      ? "s"
      : scope.kind === "case"
        ? `${scope.matchPath}/cases/${scope.caseKey}`
        : `${scope.matchPath}/default`;
  const prefix = `${expectedParent}/`;
  if (!sourceNodeId.startsWith(prefix) || !newTargetNodeId.startsWith(prefix)) {
    return workflow;
  }
  if (
    sourceNodeId.slice(prefix.length).includes("/") ||
    newTargetNodeId.slice(prefix.length).includes("/")
  ) {
    return workflow;
  }

  const aIdx = localIndex(sourceNodeId);
  const cIdx = localIndex(newTargetNodeId);
  if (aIdx < 0 || cIdx < 0) return workflow;
  if (aIdx >= chain.length || cIdx >= chain.length) return workflow;

  if (cIdx === aIdx + 1) return workflow;

  const nextChain = [...chain];
  const [moved] = nextChain.splice(cIdx, 1);
  const insertAt = cIdx < aIdx ? aIdx : aIdx + 1;
  nextChain.splice(insertAt, 0, moved);

  return setChainForScope(workflow, scope, nextChain);
}

/* ── Scope-aware append ────────────────────────────────────────────── */

/**
 * Appends `newStep` to the tail of the chain identified by `scope` and
 * returns both the updated workflow and the new step's full path (so
 * the caller can mark it as wired / disconnected, seed a position,
 * etc.). Used by:
 *
 *   - The scope-aware dock-drop: when a chip is dropped over a case or
 *     default scope region, the new step is appended into that scope
 *     and auto-wired to the prior tail.
 *   - The per-chain tail-+ button: an explicit "add a step after this
 *     chain's tail" affordance that always wires the new step in.
 *
 * Returns `null` when the scope doesn't resolve (e.g. matchPath points
 * at a non-`match` step). For a case scope whose case key isn't yet
 * present on the match step, the case is created with a single-element
 * chain — that supports the "add into empty case" path that the modal
 * editor creates when a user adds a new case key.
 */
export function appendStepToScope(
  workflow: NewWorkflow,
  scope: ChainScope,
  newStep: NewStep,
): { workflow: NewWorkflow; newPath: string } | null {
  if (scope.kind === "top-level") {
    const nextChain = [...workflow.steps, newStep];
    return {
      workflow: { ...workflow, steps: nextChain },
      newPath: `s/${workflow.steps.length}`,
    };
  }

  const matchStep = getStepAtPath(workflow.steps, scope.matchPath);
  if (!matchStep || matchStep.kind !== "match") {
    console.warn(
      `appendStepToScope: matchPath ${scope.matchPath} does not resolve to a match step`,
    );
    return null;
  }

  if (scope.kind === "case") {
    const existing = matchStep.cases[scope.caseKey] ?? [];
    const nextChain = [...existing, newStep];
    const nextWorkflow = setChainForScope(workflow, scope, nextChain);
    return {
      workflow: nextWorkflow,
      newPath: `${scope.matchPath}/cases/${scope.caseKey}/${existing.length}`,
    };
  }

  // default scope
  const existing = matchStep.default ?? [];
  const nextChain = [...existing, newStep];
  const nextWorkflow = setChainForScope(workflow, scope, nextChain);
  return {
    workflow: nextWorkflow,
    newPath: `${scope.matchPath}/default/${existing.length}`,
  };
}
