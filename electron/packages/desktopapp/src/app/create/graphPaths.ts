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
 *   - `getStepAtPath`     — resolve a path to its step (or `null`)
 *   - `updateStepAtPath`  — replace a step in-place
 *   - `deleteStepAtPath`  — remove a step from its sibling array
 *   - `reorderStepAtPath` — swap a step up/down within its siblings
 *   - `insertStepAfter`   — sibling insertion after a path (or at the
 *                           top level when path is `null`)
 *
 * All helpers are pure, return new arrays on success, and return the
 * input unchanged on invalid paths.
 */

import type { NewStep } from "./types";

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
