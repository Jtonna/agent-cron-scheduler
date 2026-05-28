import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { useState } from "react";
import { CommandPalette } from "./CommandPalette";
import type { PaletteCommand } from "./types";

const navigateCommands: PaletteCommand[] = [
  { id: "navigate:workflows", group: "Navigate", label: "Go to workflow…", hint: "Open workflows list", action: () => {} },
  { id: "navigate:runs", group: "Navigate", label: "Go to run…", hint: "Open recent runs", action: () => {} },
  { id: "navigate:dashboard", group: "Navigate", label: "Dashboard", action: () => {} },
  { id: "navigate:system-logs", group: "Navigate", label: "System Logs", action: () => {} },
];

const globalCommands: PaletteCommand[] = [
  { id: "global:new-workflow", group: "Global", label: "New Workflow", hint: "Create a workflow", action: () => {} },
  { id: "global:view-all-workflows", group: "Global", label: "View All Workflows", action: () => {} },
];

const workflowCommands: PaletteCommand[] = [
  { id: "workflow:run", group: "Workflow Actions", label: "Run Workflow", hint: "Trigger now", action: () => {} },
  { id: "workflow:run-custom", group: "Workflow Actions", label: "Run with Customizations…", action: () => {} },
  { id: "workflow:edit", group: "Workflow Actions", label: "Edit Workflow", action: () => {} },
];

const meta: Meta<typeof CommandPalette> = {
  title: "Components/CommandPalette/CommandPalette",
  component: CommandPalette,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof CommandPalette>;

/** Closed: nothing renders. Toggle in the controls to open it. */
export const Closed: Story = {
  args: {
    isOpen: false,
    commands: [...navigateCommands, ...globalCommands],
    onClose: () => {},
  },
};

/** Open with no registered commands — just static Navigate + Global groups. */
export const OpenStatic: Story = {
  render: () => {
    const [isOpen, setIsOpen] = useState(true);
    return (
      <div className="h-screen w-screen bg-surface">
        <button
          type="button"
          onClick={() => setIsOpen(true)}
          className="m-4 rounded-input bg-brand px-3 py-1.5 text-sm text-white"
        >
          Open palette
        </button>
        <CommandPalette
          isOpen={isOpen}
          onClose={() => setIsOpen(false)}
          commands={[...navigateCommands, ...globalCommands]}
        />
      </div>
    );
  },
};

/** Empty: no commands at all — palette renders the "no matches" empty state if user types. */
export const OpenEmpty: Story = {
  render: () => {
    const [isOpen, setIsOpen] = useState(true);
    return (
      <div className="h-screen w-screen bg-surface">
        <CommandPalette isOpen={isOpen} onClose={() => setIsOpen(false)} commands={[]} />
      </div>
    );
  },
};

/**
 * Open with registered workflow-scoped commands at the top — this is
 * the shape the sidebar will produce in phase 2.
 */
export const OpenWithWorkflowCommands: Story = {
  render: () => {
    const [isOpen, setIsOpen] = useState(true);
    return (
      <div className="h-screen w-screen bg-surface">
        <CommandPalette
          isOpen={isOpen}
          onClose={() => setIsOpen(false)}
          commands={[...workflowCommands, ...navigateCommands, ...globalCommands]}
        />
      </div>
    );
  },
};

/** Pre-filtered input simulation: open with a query that won't match anything. */
export const NoMatches: Story = {
  render: () => {
    const [isOpen, setIsOpen] = useState(true);
    return (
      <div className="h-screen w-screen bg-surface">
        <CommandPalette
          isOpen={isOpen}
          onClose={() => setIsOpen(false)}
          commands={[...navigateCommands, ...globalCommands]}
        />
        <p className="m-4 text-sm text-fg-muted">
          Type something like &quot;xyz123&quot; to see the empty state.
        </p>
      </div>
    );
  },
};
