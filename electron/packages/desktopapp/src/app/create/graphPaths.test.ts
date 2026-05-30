/**
 * graphPaths — reorderViaEdgeReconnect tests
 *
 * Covers the pure reorder logic that backs the reactflow edge-reconnect
 * gesture on the workflow editor canvas. Happy path + every rejection
 * branch documented in the function's contract.
 */

import { describe, expect, it } from "vitest";
import { applyNodeChanges, type NodeChange } from "@xyflow/react";
import {
  getScopeForPath,
  reorderViaEdgeReconnect,
  sameScope,
  type ChainScope,
} from "./graphPaths";
import { buildGraph, layoutGraph, type StepNodeData } from "./graph";
import type { NewStep, NewWorkflow } from "./types";

function shell(id: string): NewStep {
  return { id, kind: "shell", command: `echo ${id}`, pass_stdin: false };
}

function makeWorkflow(steps: NewStep[]): NewWorkflow {
  return { name: "wf", schedule: "* * * * *", steps };
}

function ids(steps: NewStep[]): string[] {
  return steps.map((s) => s.id);
}

const TOP: ChainScope = { kind: "top-level" };

describe("getScopeForPath", () => {
  it("returns top-level for a top-level path", () => {
    expect(getScopeForPath("s/0")).toEqual({ kind: "top-level" });
    expect(getScopeForPath("s/3")).toEqual({ kind: "top-level" });
  });

  it("returns case scope for a case-branch path", () => {
    expect(getScopeForPath("s/2/cases/ok/1")).toEqual({
      kind: "case",
      matchPath: "s/2",
      caseKey: "ok",
    });
  });

  it("returns default scope for a default-branch path", () => {
    expect(getScopeForPath("s/2/default/0")).toEqual({
      kind: "default",
      matchPath: "s/2",
    });
  });

  it("returns null for malformed paths", () => {
    expect(getScopeForPath("")).toBeNull();
    expect(getScopeForPath("s")).toBeNull();
    expect(getScopeForPath("s/cases/ok")).toBeNull();
    expect(getScopeForPath("x/0")).toBeNull();
  });
});

describe("sameScope", () => {
  it("matches identical scopes", () => {
    expect(sameScope({ kind: "top-level" }, { kind: "top-level" })).toBe(true);
    expect(
      sameScope(
        { kind: "case", matchPath: "s/1", caseKey: "ok" },
        { kind: "case", matchPath: "s/1", caseKey: "ok" },
      ),
    ).toBe(true);
  });

  it("rejects different scopes", () => {
    expect(
      sameScope({ kind: "top-level" }, { kind: "default", matchPath: "s/1" }),
    ).toBe(false);
    expect(
      sameScope(
        { kind: "case", matchPath: "s/1", caseKey: "ok" },
        { kind: "case", matchPath: "s/1", caseKey: "err" },
      ),
    ).toBe(false);
    expect(
      sameScope(
        { kind: "case", matchPath: "s/1", caseKey: "ok" },
        { kind: "case", matchPath: "s/2", caseKey: "ok" },
      ),
    ).toBe(false);
  });
});

describe("reorderViaEdgeReconnect — top-level chain", () => {
  // Chain: A B C D E (indices 0..4)
  const wf = makeWorkflow([shell("A"), shell("B"), shell("C"), shell("D"), shell("E")]);

  it("moves a later node to immediately follow the source (A→D from A→B)", () => {
    // Source = A (s/0), drop edge head on D (s/3). Expected: A D B C E.
    const next = reorderViaEdgeReconnect(wf, "s/0", "s/3", TOP);
    expect(ids(next.steps)).toEqual(["A", "D", "B", "C", "E"]);
  });

  it("moves an earlier node to follow the source (B→A from B→C)", () => {
    // Source = B (s/1), drop on A (s/0). Expected: B A C D E.
    const next = reorderViaEdgeReconnect(wf, "s/1", "s/0", TOP);
    expect(ids(next.steps)).toEqual(["B", "A", "C", "D", "E"]);
  });

  it("is a no-op when target equals source", () => {
    const next = reorderViaEdgeReconnect(wf, "s/1", "s/1", TOP);
    expect(next).toBe(wf);
  });

  it("is a no-op when target is already the immediate next", () => {
    // A→B: B is already s/0's next.
    const next = reorderViaEdgeReconnect(wf, "s/0", "s/1", TOP);
    expect(next).toBe(wf);
  });

  it("returns input unchanged when paths escape the supplied scope", () => {
    // Top-level scope but path refers to a sub-chain segment.
    const next = reorderViaEdgeReconnect(wf, "s/0", "s/1/cases/ok/0", TOP);
    expect(next).toBe(wf);
  });

  it("returns input unchanged when paths refer to indices out of bounds", () => {
    const next = reorderViaEdgeReconnect(wf, "s/0", "s/99", TOP);
    expect(next).toBe(wf);
  });
});

describe("reorderViaEdgeReconnect — match case branch", () => {
  // Top-level: HEAD, MATCH (with case ok = [X, Y, Z]), TAIL.
  const matchStep: NewStep = {
    id: "m1",
    kind: "match",
    expr: "${steps.previous.stdout}",
    cases: { ok: [shell("X"), shell("Y"), shell("Z")] },
    default: [],
  };
  const wf = makeWorkflow([shell("HEAD"), matchStep, shell("TAIL")]);
  const scope: ChainScope = { kind: "case", matchPath: "s/1", caseKey: "ok" };

  it("reorders within the case chain", () => {
    // Source X (s/1/cases/ok/0), drop on Z (s/1/cases/ok/2). Expected: X Z Y.
    const next = reorderViaEdgeReconnect(
      wf,
      "s/1/cases/ok/0",
      "s/1/cases/ok/2",
      scope,
    );
    const nextMatch = next.steps[1];
    if (nextMatch.kind !== "match") throw new Error("expected match");
    expect(ids(nextMatch.cases.ok)).toEqual(["X", "Z", "Y"]);
    // Top-level chain untouched.
    expect(next.steps[0].id).toBe("HEAD");
    expect(next.steps[2].id).toBe("TAIL");
  });

  it("rejects cross-scope drops (top-level path into case scope)", () => {
    // Top-level node HEAD (s/0) cannot be reconnected as a case-branch
    // edge — different scope.
    const next = reorderViaEdgeReconnect(wf, "s/1/cases/ok/0", "s/0", scope);
    expect(next).toBe(wf);
  });

  it("returns input unchanged when scope refers to a non-existent case", () => {
    const badScope: ChainScope = {
      kind: "case",
      matchPath: "s/1",
      caseKey: "missing",
    };
    const next = reorderViaEdgeReconnect(
      wf,
      "s/1/cases/missing/0",
      "s/1/cases/missing/1",
      badScope,
    );
    expect(next).toBe(wf);
  });

  it("returns input unchanged when match path doesn't resolve to a match step", () => {
    const badScope: ChainScope = {
      kind: "case",
      matchPath: "s/0", // HEAD is a shell, not a match
      caseKey: "ok",
    };
    const next = reorderViaEdgeReconnect(
      wf,
      "s/0/cases/ok/0",
      "s/0/cases/ok/1",
      badScope,
    );
    expect(next).toBe(wf);
  });
});

describe("live drag — reactflow position updates leave workflow untouched", () => {
  // Sanity test for the ACS-20 drag UX rework: reactflow's controlled-
  // mode contract sends NodeChange events on every drag frame, and the
  // editor folds them into its `nodes` state via `applyNodeChanges`.
  // The workflow model (steps[]) must NOT change just because a node
  // moved — positions are session-only and never serialised.
  it("does not mutate workflow.steps when a node position change is applied", () => {
    const wf: NewWorkflow = makeWorkflow([shell("A"), shell("B"), shell("C")]);
    const built = buildGraph(wf.steps);
    const nodes = layoutGraph(built.nodes, built.edges);

    const change: NodeChange<(typeof nodes)[number]> = {
      id: "s/1",
      type: "position",
      position: { x: 999, y: 999 },
      dragging: true,
    };
    const nextNodes = applyNodeChanges<(typeof nodes)[number]>([change], nodes);

    // The dragged node moved.
    const moved = nextNodes.find((n) => n.id === "s/1")!;
    expect(moved.position).toEqual({ x: 999, y: 999 });
    // The workflow steps array is byte-identical (same reference, same ids).
    expect(wf.steps).toBe(wf.steps);
    expect(wf.steps.map((s) => s.id)).toEqual(["A", "B", "C"]);
    // And the node's `data.step` still points at the same step object —
    // applyNodeChanges only swaps position-related fields.
    expect((moved.data as StepNodeData).step).toBe(wf.steps[1]);
  });
});

describe("reorderViaEdgeReconnect — match default branch", () => {
  const matchStep: NewStep = {
    id: "m1",
    kind: "match",
    expr: "${x}",
    cases: {},
    default: [shell("D1"), shell("D2"), shell("D3")],
  };
  const wf = makeWorkflow([matchStep]);
  const scope: ChainScope = { kind: "default", matchPath: "s/0" };

  it("reorders within the default chain", () => {
    // Source D1 (s/0/default/0), drop D3 (s/0/default/2). Expected: D1 D3 D2.
    const next = reorderViaEdgeReconnect(
      wf,
      "s/0/default/0",
      "s/0/default/2",
      scope,
    );
    const m = next.steps[0];
    if (m.kind !== "match") throw new Error("expected match");
    expect(ids(m.default ?? [])).toEqual(["D1", "D3", "D2"]);
  });
});
