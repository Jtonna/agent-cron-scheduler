"use client";

import { useCallback } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/apis/client";
import type { TriggerParams, WorkflowTriggerResponse } from "@/apis/types";

/**
 * Manually triggers a workflow via POST /api/workflows/{id}/trigger. Returns
 * `{ trigger, triggering, error, runId, response, reset }`. `trigger(id, params)`
 * mutates and returns the `WorkflowTriggerResponse` on success; pass `{}`
 * (the default) to use all workflow defaults. On success, invalidates the
 * recent-runs feed, cost summaries, and the workflow's own run list so the
 * sidebar and dashboard pick up the new run before SSE catches up. `reset()`
 * clears the mutation state.
 */
export function useTriggerWorkflow() {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: ({ id, params }: { id: string; params: TriggerParams }) =>
      api.triggerWorkflow(id, params),
    onSuccess: (data, variables) => {
      queryClient.invalidateQueries({ queryKey: ["runs/recent"] });
      queryClient.invalidateQueries({ queryKey: ["cost/workflows"] });
      queryClient.invalidateQueries({ queryKey: ["jobs", variables.id, "runs"] });
    },
  });

  const { mutateAsync, reset } = mutation;

  const trigger = useCallback(
    (id: string, params: TriggerParams = {}): Promise<WorkflowTriggerResponse> =>
      mutateAsync({ id, params }),
    [mutateAsync]
  );

  return {
    trigger,
    triggering: mutation.isPending,
    error: mutation.error instanceof Error ? mutation.error.message : null,
    runId: mutation.data?.run_id ?? null,
    response: mutation.data ?? null,
    reset,
  };
}
