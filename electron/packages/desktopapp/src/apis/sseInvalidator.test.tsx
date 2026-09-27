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
