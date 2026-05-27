"use client";

import { useMutation, useQueryClient } from "@tanstack/react-query";
import { api } from "@/apis/client";

/**
 * Sends a kill signal to an in-flight run. On success, invalidates the
 * corresponding `["runs", runId]` cache entry so the run summary refetches
 * and reflects the new status. The backend also emits a `run_completed`
 * SSE event with status `Killed` which SSEQueryBridge picks up — this
 * invalidation just shortens the gap between request and UI update.
 */
export function useKillRun() {
  const queryClient = useQueryClient();

  const mutation = useMutation({
    mutationFn: (runId: string) => api.killRun(runId),
    onSuccess: (_data, runId) => {
      queryClient.invalidateQueries({ queryKey: ["runs", runId] });
    },
  });

  return {
    kill: (runId: string) => mutation.mutateAsync(runId),
    killing: mutation.isPending,
    error: mutation.error instanceof Error ? mutation.error.message : null,
  };
}
