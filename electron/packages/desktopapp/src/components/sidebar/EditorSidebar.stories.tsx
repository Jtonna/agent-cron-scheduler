/**
 * EditorSidebar stories.
 *
 * Showcases the editor's left rail in both create and edit modes,
 * plus the editing-name, error, empty-steps disabled, and the at-rest
 * "editable affordances visible" states. Each story wraps the sidebar
 * in a tiny harness that owns the controlled fields so typing actually
 * persists across keystrokes.
 *
 * The sidebar now hosts a `SidebarSearchTrigger` which calls
 * `useCommandPalette()`. That hook throws outside its provider, so the
 * Storybook meta wraps every story in a `CommandPaletteContext.Provider`
 * with a no-op fake — same pattern used by `JobDetailSidebar.stories.tsx`.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { EditorSidebar, type EditorSidebarMode } from "./EditorSidebar";
import { CommandPaletteContext } from "@/components/command-palette/CommandPaletteContext";

const fakePaletteCtx = {
  isOpen: false,
  open: () => {},
  close: () => {},
  toggle: () => {},
  registerCommands: () => "reg-0",
  unregisterCommands: () => {},
};

const meta: Meta<typeof EditorSidebar> = {
  title: "Sidebar/EditorSidebar",
  component: EditorSidebar,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <CommandPaletteContext.Provider value={fakePaletteCtx}>
        <Story />
      </CommandPaletteContext.Provider>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof EditorSidebar>;

interface HarnessOverrides {
  mode?: EditorSidebarMode;
  initialName?: string;
  initialSchedule?: string;
  initialTimezone?: string;
  initialEnabled?: boolean;
  stepCount?: number;
  busy?: boolean;
  errorText?: string | null;
  autoFocusName?: boolean;
}

function Harness({
  mode = { kind: "create" },
  initialName = "",
  initialSchedule = "0 9 * * *",
  initialTimezone = "America/Los_Angeles",
  initialEnabled = true,
  stepCount = 3,
  busy = false,
  errorText = null,
  autoFocusName = false,
}: HarnessOverrides) {
  const [name, setName] = useState(initialName);
  const [schedule, setSchedule] = useState(initialSchedule);
  const [timezone, setTimezone] = useState(initialTimezone);
  const [enabled, setEnabled] = useState(initialEnabled);

  return (
    <div
      className="grid grid-cols-[var(--width-sidebar)_1fr] h-screen bg-surface"
      ref={(el) => {
        if (!el || !autoFocusName) return;
        requestAnimationFrame(() => {
          const btn = el.querySelector<HTMLButtonElement>(
            'button[aria-label="Edit workflow name"]',
          );
          btn?.click();
        });
      }}
    >
      <div className="border-r border-border-subtle h-full">
        <EditorSidebar
          mode={mode}
          name={name}
          onNameChange={setName}
          schedule={schedule}
          onScheduleChange={setSchedule}
          timezone={timezone}
          onTimezoneChange={setTimezone}
          enabled={enabled}
          onEnabledChange={setEnabled}
          stepCount={stepCount}
          onSubmit={() => {}}
          busy={busy}
          errorText={errorText}
        />
      </div>
      <div className="bg-surface-tertiary flex items-center justify-center text-fg-muted text-sm">
        Canvas placeholder
      </div>
    </div>
  );
}

export const CreateMode: Story = {
  render: () => <Harness initialName="my-new-workflow" stepCount={2} />,
};

export const EditMode: Story = {
  render: () => (
    <Harness
      mode={{ kind: "edit", workflowId: "wf_existing_123" }}
      initialName="weather-greeter"
      initialSchedule="0 8 * * *"
      initialTimezone="America/Los_Angeles"
      stepCount={4}
    />
  ),
};

export const EditingName: Story = {
  render: () => (
    <Harness
      initialName="rename-me"
      stepCount={1}
      autoFocusName
    />
  ),
};

/**
 * IdleEditableFields — at-rest view that demonstrates the dashed-border
 * editable affordance on the name, cron, and timezone fields without any
 * hover or focus. Use this to verify that fields look like fields even
 * when nothing is being interacted with.
 */
export const IdleEditableFields: Story = {
  render: () => (
    <Harness
      mode={{ kind: "edit", workflowId: "wf_idle" }}
      initialName="daily-digest"
      initialSchedule="0 9 * * 1-5"
      initialTimezone="America/New_York"
      stepCount={3}
    />
  ),
};

export const WithSubmissionError: Story = {
  render: () => (
    <Harness
      mode={{ kind: "edit", workflowId: "wf_err" }}
      initialName="broken-workflow"
      stepCount={2}
      errorText="Validation failed: step 1 must have a command."
    />
  ),
};

export const EmptyStepsDisabled: Story = {
  render: () => (
    <Harness initialName="no-steps-yet" stepCount={0} />
  ),
};
