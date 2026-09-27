"use client";

import { useCallback, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { useTriggerWorkflow } from "@/apis/useTriggerWorkflow";
import { getActiveRunId } from "@/apis/client";
import type { TriggerParams } from "@/apis/types";

/**
 * Client hook that triggers a workflow and navigates to the run page on success.
 * If a 409 concurrent_run_active error occurs, displays an error with a link to
 * view the already-running run instead of navigating.
 *
 * Returns { run, running, error, activeRunId, reset, openRun }:
 *   - run(params?): triggers the workflow; on success navigates to /workflows/{id}/runs/{runId}
 *   - running: boolean indicating if a trigger is in progress
 *   - error: error message from the trigger hook, or null
 *   - activeRunId: the run_id of the already-active run (only set on 409 error)
 *   - reset: clears both the hook state and the local activeRunId
 *   - openRun(runId): navigates to /workflows/{id}/runs/{runId}
 */
export function useRunWorkflowAndOpen(workflowId: string) {
  const router = useRouter();
  const { trigger, triggering, error: triggerError, reset: resetHook } = useTriggerWorkflow();

  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const inFlightRef = useRef(false);

  const run = useCallback(
    async (params: TriggerParams = {}) => {
      if (inFlightRef.current) return;
      inFlightRef.current = true;
      setActiveRunId(null);

      try {
        const response = await trigger(workflowId, params);
        router.push(`/workflows/${workflowId}/runs/${response.run_id}`);
      } catch (e) {
        const runId = getActiveRunId(e);
        if (runId) {
          setActiveRunId(runId);
        }
      } finally {
        inFlightRef.current = false;
      }
    },
    [workflowId, trigger, router]
  );

  const openRun = useCallback(
    (runId: string) => {
      router.push(`/workflows/${workflowId}/runs/${runId}`);
    },
    [workflowId, router]
  );

  const reset = useCallback(() => {
    resetHook();
    setActiveRunId(null);
  }, [resetHook]);

  return {
    run,
    running: triggering,
    error: triggerError,
    activeRunId,
    reset,
    openRun,
  };
}
