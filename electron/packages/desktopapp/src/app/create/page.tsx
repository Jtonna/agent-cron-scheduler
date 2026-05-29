"use client";

/**
 * /create page
 *
 * Prototype graph-based workflow builder. Renders the navbar + the
 * WorkflowGraphEditor. The editor owns its own state — the page just
 * seeds the initial NewWorkflow with one empty shell step so the canvas
 * isn't blank on first load.
 */

import { Navbar } from "@/components/navbar/Navbar";
import { WorkflowGraphEditor } from "./WorkflowGraphEditor";
import { makeDefaultStep, type NewWorkflow } from "./types";

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
  return (
    <div className="min-h-screen bg-surface text-fg flex flex-col">
      <Navbar />
      <WorkflowGraphEditor initialWorkflow={seedWorkflow()} />
    </div>
  );
}
