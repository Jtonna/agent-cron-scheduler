"use client";

/**
 * /create page
 *
 * Hosts the whiteboard-style workflow builder. Mounts the Navbar on top,
 * then a row with `EditorSidebar` (left rail; identity, schedule, save)
 * and `WorkflowGraphEditor` (canvas; steps + bottom-centre dock).
 *
 * The page owns the editor's identity state — `name`, `schedule`,
 * `timezone`, `enabled` — and the submission wiring (`useCreateWorkflow`
 * + navigation) so both the sidebar (which surfaces those fields + the
 * Save button) and the editor (which serialises the workflow on submit)
 * stay in sync without either having to know about the other. The
 * editor reports back via `onWorkflowChange` so the page always has a
 * fresh `NewWorkflow` snapshot to submit.
 */

import { useCallback, useRef, useState } from "react";
import { useRouter } from "next/navigation";
import { Navbar } from "@/components/navbar/Navbar";
import { EditorSidebar } from "@/components/sidebar/EditorSidebar";
import { WorkflowGraphEditor } from "./WorkflowGraphEditor";
import { makeDefaultStep, type NewWorkflow } from "./types";
import { serialiseWorkflow } from "./workflowSerialization";
import { useCreateWorkflow } from "@/apis/useCreateWorkflow";

function seedWorkflow(): NewWorkflow {
  return {
    name: "",
    schedule: "0 9 * * *",
    timezone: "America/Los_Angeles",
    enabled: true,
    steps: [makeDefaultStep("shell")],
  };
}

export default function CreatePage() {
  const router = useRouter();
  const [seed] = useState<NewWorkflow>(() => seedWorkflow());

  // Sidebar-owned identity fields.
  const [name, setName] = useState(seed.name);
  const [schedule, setSchedule] = useState(seed.schedule);
  const [timezone, setTimezone] = useState(seed.timezone ?? "");
  const [enabled, setEnabled] = useState(seed.enabled ?? true);

  // Latest snapshot of the editor's workflow (mostly tracks `steps`).
  const workflowRef = useRef<NewWorkflow>(seed);
  const [stepCount, setStepCount] = useState(seed.steps.length);
  const handleWorkflowChange = useCallback((next: NewWorkflow) => {
    workflowRef.current = next;
    setStepCount(next.steps.length);
  }, []);

  const { create, creating, error: createError } = useCreateWorkflow();
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const handleSubmit = useCallback(async () => {
    setSubmissionError(null);
    try {
      const body = serialiseWorkflow(workflowRef.current);
      const created = await create(body);
      router.push(`/workflows/${created.id}`);
    } catch (e) {
      setSubmissionError(e instanceof Error ? e.message : "Unknown error");
    }
  }, [create, router]);

  const errorText = submissionError ?? createError ?? null;

  return (
    <div className="min-h-screen bg-surface text-fg flex flex-col">
      <Navbar />
      <div className="grid grid-cols-[var(--width-sidebar)_1fr] flex-1 min-h-0">
        <div className="border-r border-border-subtle h-[calc(100vh-var(--height-navbar))]">
          <EditorSidebar
            mode={{ kind: "create" }}
            name={name}
            onNameChange={setName}
            schedule={schedule}
            onScheduleChange={setSchedule}
            timezone={timezone}
            onTimezoneChange={setTimezone}
            enabled={enabled}
            onEnabledChange={setEnabled}
            stepCount={stepCount}
            onSubmit={handleSubmit}
            busy={creating}
            errorText={errorText}
          />
        </div>
        <div className="flex flex-col h-[calc(100vh-var(--height-navbar))]">
          <WorkflowGraphEditor
            mode="create"
            initialWorkflow={seed}
            name={name}
            schedule={schedule}
            timezone={timezone}
            enabled={enabled}
            onWorkflowChange={handleWorkflowChange}
          />
        </div>
      </div>
    </div>
  );
}
