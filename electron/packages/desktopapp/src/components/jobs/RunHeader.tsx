import type { JobRun } from "@/apis/types";
import {
  JobStateIndicator,
  apiStatusToJobState,
} from "@/components/ui/JobStateIndicator";

/**
 * RunHeader
 *
 * Identity block atop the run-detail page (`/workflows/[id]/runs/[runId]`).
 * Workflow name dominates as an h1; a secondary meta line beneath it
 * leads with the status label and then the run id. The back link lives
 * in the sidebar, the kill button lives in the log viewer's top bar,
 * and the stat strip lives below.
 */

export interface RunHeaderProps {
  workflowName: string;
  runId: string;
  status: JobRun["status"];
}

export function RunHeader({ workflowName, runId, status }: RunHeaderProps) {
  const state = apiStatusToJobState(status);
  return (
    <div className="flex flex-col gap-1 min-w-0">
      <h1
        className="text-2xl font-extrabold tracking-tight text-fg truncate"
        title={workflowName}
      >
        {workflowName}
      </h1>
      <div className="flex items-center gap-2 min-w-0">
        <JobStateIndicator state={state} variant="label" />
        <span className="text-fg-faint">·</span>
        <span className="font-mono text-xs text-fg-muted truncate" title={runId}>
          {runId}
        </span>
      </div>
    </div>
  );
}
