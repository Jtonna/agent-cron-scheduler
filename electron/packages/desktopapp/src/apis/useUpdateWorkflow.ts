"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, type WorkflowUpdate } from "@/apis/client";

/**
 * Patches a workflow definition. On success, invalidates the `["jobs"]`
 * list cache and the `["jobs", id]` single-workflow cache so the detail
 * page and any list views refetch the updated record. The backend also
 * emits a `job_changed` SSE event picked up by SSEQueryBridge; these
 * invalidations just shorten the gap between request and UI update.
 */
export function useUpdateWorkflow(id: string) {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: (update: WorkflowUpdate) => api.updateWorkflow(id, update),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["jobs"] });
      queryClient.invalidateQueries({ queryKey: ["jobs", id] });
    },
  });

  return {
    update: (next: WorkflowUpdate) => mutation.mutateAsync(next),
    updating: mutation.isPending,
    error: mutation.error instanceof Error ? mutation.error.message : null,
  };
}
