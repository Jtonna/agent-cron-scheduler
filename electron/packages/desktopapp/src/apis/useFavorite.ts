"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Toggle the `is_favorited` flag on a workflow.
 *
 * Wraps `POST /api/workflows/{id}/favorite` and
 * `DELETE /api/workflows/{id}/favorite` in a single hook so callers can do
 * `const { favorite, unfavorite } = useFavorite()` and pick the action based
 * on the current state. On success, invalidates `["jobs"]` so any list
 * filtered/sorted by `is_favorited` re-renders. The backend also broadcasts
 * a `workflow_changed` SSE event that SSEQueryBridge picks up — this
 * invalidation just shortens the gap between request and UI update.
 *
 * `isPending` is true while either mutation is in flight, suitable for a
 * subtle visual cue on the toggle (e.g. reduced opacity).
 */
export function useFavorite() {
  const queryClient = useQueryClient();

  const favoriteMutation = useMutation({
    mutationFn: (id: string) => api.favoriteWorkflow(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["jobs"] });
    },
  });

  const unfavoriteMutation = useMutation({
    mutationFn: (id: string) => api.unfavoriteWorkflow(id),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["jobs"] });
    },
  });

  const error =
    favoriteMutation.error ?? unfavoriteMutation.error;

  return {
    favorite: (id: string) => favoriteMutation.mutateAsync(id),
    unfavorite: (id: string) => unfavoriteMutation.mutateAsync(id),
    isPending: favoriteMutation.isPending || unfavoriteMutation.isPending,
    error: error instanceof Error ? error.message : null,
  };
}
