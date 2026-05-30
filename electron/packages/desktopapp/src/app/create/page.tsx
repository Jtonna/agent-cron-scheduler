"use client";

/**
 * /create page
 *
 * Hosts the whiteboard-style workflow builder. Renders the Navbar,
 * then a `CanvasBreadcrumb` whose final crumb is the EDITABLE workflow
 * name (click-to-edit, replacing the old on-canvas inline title), then
 * mounts `WorkflowGraphEditor` in `create` mode with a single seeded
 * shell step so the canvas isn't blank on first load.
 *
 * Name state is lifted to this page so the breadcrumb (display + edit
 * surface) and the editor (which needs the name for submit) can both
 * read and write the same value. The editor accepts `name` +
 * `onNameChange` controlled-input style for that reason.
 */

import { useCallback, useState } from "react";
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
  const [seed] = useState<NewWorkflow>(() => seedWorkflow());
  const [name, setName] = useState<string>(seed.name);
  const handleNameChange = useCallback((n: string) => setName(n), []);

  return (
    <div className="min-h-screen bg-surface text-fg flex flex-col">
      <Navbar />
      <CanvasBreadcrumb
        crumbs={[
          { label: "Workflows", href: "/workflows" },
          {
            kind: "editable",
            value: name,
            onChange: handleNameChange,
            placeholder: "New workflow",
            ariaLabel: "Edit workflow name",
          },
        ]}
      />
      <WorkflowGraphEditor
        mode="create"
        initialWorkflow={seed}
        name={name}
        onNameChange={handleNameChange}
      />
    </div>
  );
}
