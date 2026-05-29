"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/apis/client";
import type { Job } from "@/apis/types";

/**
 * useCreateWorkflow
 *
 * Wraps `POST /api/workflows` in a react-query mutation. Mirrors
 * `useUpdateWorkflow` but for the create flow on `/create`.
 *
 * On success it invalidates the `["jobs"]` list so the workflow shows
 * up immediately in list views; the caller is expected to navigate the
 * user onward to the new workflow's detail page.
 */
export function useCreateWorkflow() {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: (body: Record<string, unknown>): Promise<Job> => api.createWorkflow(body),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ["jobs"] });
    },
  });

  return {
    create: (body: Record<string, unknown>) => mutation.mutateAsync(body),
    creating: mutation.isPending,
    error: mutation.error instanceof Error ? mutation.error.message : null,
    reset: () => mutation.reset(),
  };
}
