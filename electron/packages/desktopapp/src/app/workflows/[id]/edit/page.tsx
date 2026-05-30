"use client";

/**
 * Edit workflow page (`/workflows/[id]/edit`).
 *
 * Fetches the workflow via `useJob`, then hands it to the shared
 * `WorkflowGraphEditor` in `edit` mode. The workflow name is hoisted
 * up here so the `CanvasBreadcrumb` can render it as an editable
 * crumb (replacing the old inline-on-canvas heading).
 *
 * The displayed name is derived: `null` means "use whatever the job
 * came back with"; once the user edits the field we store the edited
 * value (including empty string) in state. This avoids a setState-in-
 * effect seed pattern and keeps the source-of-truth lookup local.
 *
 * Rendering loading/error UI inline (rather than inside the editor)
 * keeps the editor agnostic of how its `initialWorkflow` was produced —
 * the editor only ever sees a fully-formed `Job`.
 */

import { use, useCallback, useState } from "react";
import { Loader2 } from "lucide-react";
import { Navbar } from "@/components/navbar/Navbar";
import { useJob } from "@/apis/useJob";
import { WorkflowGraphEditor } from "@/app/create/WorkflowGraphEditor";
import { CanvasBreadcrumb } from "@/app/create/CanvasBreadcrumb";

export default function EditWorkflowPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const { job, loading, error } = useJob(id);
  // `null` = user hasn't touched the field yet; fall back to job.name.
  // Anything else (including "") = user-edited value.
  const [editedName, setEditedName] = useState<string | null>(null);
  const handleNameChange = useCallback((n: string) => setEditedName(n), []);
  const displayedName = editedName ?? job?.name ?? "";

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
        <>
          <CanvasBreadcrumb
            crumbs={[
              { label: "Workflows", href: "/workflows" },
              {
                kind: "editable",
                value: displayedName,
                onChange: handleNameChange,
                placeholder: id,
                ariaLabel: "Edit workflow name",
              },
              { label: "Edit" },
            ]}
          />
          <WorkflowGraphEditor
            mode="edit"
            workflowId={id}
            initialWorkflow={job}
            name={displayedName}
            onNameChange={handleNameChange}
          />
        </>
      )}
    </div>
  );
}
