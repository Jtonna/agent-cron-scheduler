"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api, ApiError } from "@/apis/client";

/**
 * Soft-deletes a workflow.
 *
 * On success, `onDeleted` fires first — the caller is expected to navigate
 * away from the (now-gone) workflow detail page in that callback — before
 * any cache entries are torn down. This ordering matters: if the caches
 * were cleared first, a still-mounted detail page would refetch
 * `["jobs", id]` and hit a 404 before the navigation completes.
 *
 * After `onDeleted`:
 * - `["jobs", id]` is removed (a prefix match, so it also clears that
 *   workflow's runs query under `["jobs", id, "runs", ...]`)
 * - `["workflows", id, "cost"]` is removed
 * - `["jobs"]` (the list) is invalidated so it refetches without the
 *   deleted workflow
 * - `["cost/workflows"]` (the global cost summary) is invalidated
 *
 * Race note: the backend broadcasts the `workflow_changed`/`deleted` SSE
 * event *before* it returns the 204 (see workflow_routes.rs), so
 * SSEQueryBridge can invalidate `["jobs"]` while the detail page for the
 * workflow being deleted is still mounted, triggering a refetch of
 * `["jobs", id]` that 404s. This is not actually a problem here: React
 * Query's default `retry: 1` (with backoff, see providers.tsx) delays that
 * refetch's failure long enough for `onSuccess` below to remove the query
 * and for `onDeleted`'s `router.replace` to navigate away first.
 */
export function useDeleteWorkflow(options?: { onDeleted?: (id: string) => void }) {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: (id: string) => api.deleteWorkflow(id),
    onSuccess: (_data, id) => {
      options?.onDeleted?.(id);
      queryClient.removeQueries({ queryKey: ["jobs", id] });
      queryClient.removeQueries({ queryKey: ["workflows", id, "cost"] });
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
