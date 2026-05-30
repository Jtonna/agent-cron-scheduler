"use client";

/**
 * /create page
 *
 * Hosts the whiteboard-style workflow builder. Renders the Navbar
 * (with a tiny breadcrumb chip floating just under it identifying
 * the route), then mounts `WorkflowGraphEditor` in `create` mode with
 * a single seeded shell step so the canvas isn't blank on first load.
 */

import { Navbar } from "@/components/navbar/Navbar";
import { WorkflowGraphEditor } from "./WorkflowGraphEditor";
import { CanvasBreadcrumb } from "./CanvasBreadcrumb";
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
      <CanvasBreadcrumb crumbs={[{ label: "Workflows", href: "/workflows" }, { label: "New workflow" }]} />
      <WorkflowGraphEditor mode="create" initialWorkflow={seedWorkflow()} />
    </div>
  );
}
