"use client";

/**
 * Edit workflow page (`/workflows/[id]/edit`).
 *
 * Fetches the workflow via `useJob`, then renders the shared editor
 * surface: `EditorSidebar` on the left (identity + schedule + save),
 * `WorkflowGraphEditor` (in `edit` mode) on the right.
 *
 * Sidebar-owned identity state (name, schedule, timezone, enabled) is
 * lifted to this page and seeded from the fetched job. The displayed
 * name uses the "first edit replaces the fetched value" pattern: until
 * the user touches the input we render whatever the job came back with;
 * after the first edit (including clearing to `""`) the user's value
 * wins. The submission wiring (`useUpdateWorkflow` + navigation) also
 * lives here so the sidebar can call into it via `onSubmit`.
 */

import { use, useCallback, useMemo, useRef, useState } from "react";
import { Loader2 } from "lucide-react";
import { useRouter } from "next/navigation";
import { Navbar } from "@/components/navbar/Navbar";
import { useJob } from "@/apis/useJob";
import { useUpdateWorkflow } from "@/apis/useUpdateWorkflow";
import { EditorSidebar } from "@/components/sidebar/EditorSidebar";
import { WorkflowGraphEditor } from "@/app/create/WorkflowGraphEditor";
import {
  jobToNewWorkflow,
  serialiseWorkflow,
} from "@/app/create/workflowSerialization";
import type { NewWorkflow } from "@/app/create/types";
import type { Job } from "@/apis/types";

export default function EditWorkflowPage({
  params,
}: {
  params: Promise<{ id: string }>;
}) {
  const { id } = use(params);
  const router = useRouter();
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
        <LoadedEditor workflowId={id} job={job} router={router} />
      )}
    </div>
  );
}

interface LoadedEditorProps {
  workflowId: string;
  job: Job;
  router: ReturnType<typeof useRouter>;
}

/**
 * Split out so we can seed `useState` from the fetched job exactly once
 * (the outer page can render the loader before the job arrives).
 */
function LoadedEditor({ workflowId, job, router }: LoadedEditorProps) {
  // Seed once from the fetched job. `useMemo` over `useRef.current` so the
  // react-hooks/refs lint stays happy with the initial-state derivations
  // below. Empty deps array because the parent only mounts `LoadedEditor`
  // after `job` resolves and never swaps the job out in place.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const seed = useMemo<NewWorkflow>(() => jobToNewWorkflow(job), []);

  const [name, setName] = useState(seed.name);
  const [schedule, setSchedule] = useState(seed.schedule);
  const [timezone, setTimezone] = useState(seed.timezone ?? "");
  const [enabled, setEnabled] = useState(seed.enabled ?? true);

  const workflowRef = useRef<NewWorkflow>(seed);
  const [stepCount, setStepCount] = useState(seed.steps.length);
  const [disconnectedCount, setDisconnectedCount] = useState(0);
  const handleWorkflowChange = useCallback((next: NewWorkflow) => {
    workflowRef.current = next;
    setStepCount(next.steps.length);
  }, []);

  const {
    update,
    updating,
    error: updateError,
  } = useUpdateWorkflow(workflowId);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const handleSubmit = useCallback(async () => {
    setSubmissionError(null);
    try {
      const body = serialiseWorkflow(workflowRef.current);
      await update(body);
      router.push(`/workflows/${workflowId}`);
    } catch (e) {
      setSubmissionError(e instanceof Error ? e.message : "Unknown error");
    }
  }, [router, update, workflowId]);

  const errorText = submissionError ?? updateError ?? null;

  return (
    <div className="grid grid-cols-[var(--width-sidebar)_1fr] flex-1 min-h-0">
      <div className="border-r border-border-subtle h-[calc(100vh-var(--height-navbar))]">
        <EditorSidebar
          mode={{ kind: "edit", workflowId }}
          name={name}
          onNameChange={setName}
          schedule={schedule}
          onScheduleChange={setSchedule}
          timezone={timezone}
          onTimezoneChange={setTimezone}
          enabled={enabled}
          onEnabledChange={setEnabled}
          stepCount={stepCount}
          disconnectedCount={disconnectedCount}
          onSubmit={handleSubmit}
          busy={updating}
          errorText={errorText}
        />
      </div>
      <div className="flex flex-col h-[calc(100vh-var(--height-navbar))]">
        <WorkflowGraphEditor
          mode="edit"
          workflowId={workflowId}
          initialWorkflow={job}
          name={name}
          schedule={schedule}
          timezone={timezone}
          enabled={enabled}
          onWorkflowChange={handleWorkflowChange}
          onDisconnectedCountChange={setDisconnectedCount}
        />
      </div>
    </div>
  );
}
