"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "@/apis/client";

/**
 * Soft-deletes a workflow.
 *
 * On success, `onDeleted` fires first. The caller is expected to navigate
 * away from the (now-gone) workflow detail page in that callback. After it:
 * - `["jobs"]` (the list only, `exact: true`) is invalidated so it refetches
 *   without the deleted workflow
 * - `["cost/workflows"]` (the global cost summary) is invalidated
 *
 * Per-id caches (`["jobs", id]`, `["jobs", id, "runs", ...]`,
 * `["workflows", id, "cost"]`) are deliberately NOT removed or invalidated.
 * `router.replace` is async, so the detail page's observers are still
 * mounted here; removing their queries makes the observers recreate and
 * refetch them immediately, and every such refetch 404s. The stale entries
 * are dropped by gcTime once unobserved, and a later visit to the deleted id
 * refetches and renders the normal not-found state.
 *
 * Race note: the backend broadcasts the `workflow_changed`/`deleted` SSE
 * event before it returns the 204 (see workflow_routes.rs). SSEQueryBridge
 * therefore handles "deleted" with an exact list invalidation too, never a
 * `["jobs"]` prefix invalidation, for the same reason.
 */
export function useDeleteWorkflow(options?: { onDeleted?: (id: string) => void }) {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: (id: string) => api.deleteWorkflow(id),
    onSuccess: (_data, id) => {
      options?.onDeleted?.(id);
      queryClient.invalidateQueries({ queryKey: ["jobs"], exact: true });
      queryClient.invalidateQueries({ queryKey: ["cost/workflows"] });
    },
  });

  const error = mutation.error;

  return {
    deleteWorkflow: (id: string) => mutation.mutateAsync(id),
    deleting: mutation.isPending,
    error: error instanceof Error ? error.message : null,
    errorCode: error instanceof ApiError ? error.code : null,
    errorStatus: error instanceof ApiError ? error.status : null,
    reset: mutation.reset,
  };
}
