import { describe, expect, it, vi } from "vitest";
import { render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
type SSEEvent = { type: string; data: string };

let emit: (event: SSEEvent) => void = () => {};

vi.mock("./sse", () => ({
  useSSEEvents: (cb: (event: SSEEvent) => void) => {
    emit = cb;
  },
}));

import { SSEQueryBridge } from "./sseInvalidator";

function setup() {
  const queryClient = new QueryClient();
  const spy = vi.spyOn(queryClient, "invalidateQueries");
  render(
    <QueryClientProvider client={queryClient}>
      <SSEQueryBridge />
    </QueryClientProvider>
  );
  return spy;
}

function workflowChanged(changeKind: string): SSEEvent {
  return {
    type: "workflow_changed",
    data: JSON.stringify({
      type: "WorkflowChanged",
      workflow_id: "wf-1",
      version: 2,
      change_kind: changeKind,
    }),
  };
}

describe("SSEQueryBridge workflow_changed", () => {
  it("on deleted: invalidates only the exact list and the cost summary", () => {
    const spy = setup();
    emit(workflowChanged("deleted"));
    expect(spy).toHaveBeenCalledTimes(2);
    expect(spy).toHaveBeenCalledWith({ queryKey: ["jobs"], exact: true });
    expect(spy).toHaveBeenCalledWith({ queryKey: ["cost/workflows"] });
  });

  it("on updated: invalidates the whole jobs prefix", () => {
    const spy = setup();
    emit(workflowChanged("updated"));
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith({ queryKey: ["jobs"] });
  });

  it("on unparseable payload: falls back to the jobs prefix", () => {
    const spy = setup();
    emit({ type: "workflow_changed", data: "not json" });
    expect(spy).toHaveBeenCalledWith({ queryKey: ["jobs"] });
  });
});

describe("SSEQueryBridge step_started", () => {
  it("with valid run_id: invalidates the exact run key", () => {
    const spy = setup();
    emit({
      type: "step_started",
      data: JSON.stringify({
        run_id: "r1",
        workflow_id: "wf-1",
        step_index: 0,
        step_id: "step-1",
        kind: "Task",
        started_at: "2026-09-27T00:00:00Z",
      }),
    });
    expect(spy).toHaveBeenCalledTimes(1);
    expect(spy).toHaveBeenCalledWith({
      queryKey: ["runs", "r1"],
      exact: true,
    });
  });

  it("with unparseable data: does not call invalidateQueries", () => {
    const spy = setup();
    emit({ type: "step_started", data: "not json" });
    expect(spy).not.toHaveBeenCalled();
  });

  it("with no run_id: does not call invalidateQueries", () => {
    const spy = setup();
    emit({
      type: "step_started",
      data: JSON.stringify({
        workflow_id: "wf-1",
        step_index: 0,
      }),
    });
    expect(spy).not.toHaveBeenCalled();
  });

  it("does not refetch the log buffer (exact: true only fetches the run key)", () => {
    const queryClient = new QueryClient();
    render(
      <QueryClientProvider client={queryClient}>
        <SSEQueryBridge />
      </QueryClientProvider>
    );
    // Pre-populate the log cache
    queryClient.setQueryData(["runs", "r1", "log"], ["line 1", "line 2"]);
    // Emit step_started
    emit({
      type: "step_started",
      data: JSON.stringify({ run_id: "r1" }),
    });
    // Verify ["runs", "r1", "log"] is not invalidated
    expect(
      queryClient.getQueryState(["runs", "r1", "log"])?.isInvalidated
    ).toBe(false);
  });
});

describe("SSEQueryBridge step_output", () => {
  it("does not call invalidateQueries (handled inline by consumers)", () => {
    const spy = setup();
    emit({
      type: "step_output",
      data: JSON.stringify({
        run_id: "r1",
        step_index: 0,
        text: "hello",
      }),
    });
    expect(spy).not.toHaveBeenCalled();
  });
});
