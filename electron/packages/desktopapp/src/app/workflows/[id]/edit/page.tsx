"use client";

/**
 * Edit workflow page (`/workflows/[id]/edit`).
 *
 * Fetches the workflow via `useJob`, then hands it to the shared
 * `WorkflowGraphEditor` in `edit` mode. Rendering loading/error UI inline
 * (rather than inside the editor itself) keeps the editor agnostic of how
 * its `initialWorkflow` was produced — the editor only ever sees a
 * fully-formed `Job`.
 */

import { use } from "react";
import { Loader2 } from "lucide-react";
import { Navbar } from "@/components/navbar/Navbar";
import { useJob } from "@/apis/useJob";
import { WorkflowGraphEditor } from "@/app/create/WorkflowGraphEditor";

export default function EditWorkflowPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { job, loading, error } = useJob(id);

  return (
    <div className="min-h-screen bg-surface text-fg flex flex-col">
      <Navbar />
      {loading && !job ? (
        <div className="flex flex-1 items-center justify-center gap-2 text-fg-muted text-sm">
          <Loader2 size={16} className="animate-spin text-fg-subtle" />
          Loading workflow…
        </div>
      ) : error ? (
        <div className="mx-auto max-w-2xl px-8 py-10">
          <div className="rounded-card border border-status-failed-border bg-status-failed-bg p-4 text-status-failed text-sm">
            Failed to load workflow: {error}
          </div>
        </div>
      ) : !job ? (
        <div className="mx-auto max-w-2xl px-8 py-10">
          <p className="text-fg-muted text-sm">
            Workflow <span className="font-mono">{id}</span> doesn&apos;t exist or has been deleted.
          </p>
        </div>
      ) : (
        <WorkflowGraphEditor mode="edit" workflowId={id} initialWorkflow={job} />
      )}
    </div>
  );
}
