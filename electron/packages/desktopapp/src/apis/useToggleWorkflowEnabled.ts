"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Toggles a workflow's `enabled` flag via PATCH /api/workflows/{id}. The
 * caller passes the current value and the hook flips it. On success,
 * invalidates the global `["jobs"]` list and the single-workflow
 * `["jobs", id]` cache so the UI reflects the new state immediately. The
 * backend also emits a `workflow_changed` SSE event which SSEQueryBridge
 * picks up — this invalidation just shortens the gap between request and
 * UI update.
 */
export function useToggleWorkflowEnabled() {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: ({ id, currentEnabled }: { id: string; currentEnabled: boolean }) =>
      api.toggleWorkflowEnabled(id, !currentEnabled),
    onSuccess: (_data, variables) => {
      queryClient.invalidateQueries({ queryKey: ["jobs"] });
      queryClient.invalidateQueries({ queryKey: ["jobs", variables.id] });
    },
  });

  return {
    toggle: (id: string, currentEnabled: boolean) =>
      mutation.mutateAsync({ id, currentEnabled }),
    toggling: mutation.isPending,
    error: mutation.error instanceof Error ? mutation.error.message : null,
  };
}
