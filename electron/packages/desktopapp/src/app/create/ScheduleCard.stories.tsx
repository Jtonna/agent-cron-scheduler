/**
 * ScheduleCard stories — the floating top-centre card that holds the
 * cron pill, timezone, enabled toggle and primary action.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { ScheduleCard } from "./ScheduleCard";
import type { NewWorkflow } from "./types";

const meta: Meta<typeof ScheduleCard> = {
  title: "Pages/CreateWorkflow/ScheduleCard",
  component: ScheduleCard,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof ScheduleCard>;

function Harness({ initial, submitLabel }: { initial: NewWorkflow; submitLabel: string }) {
  const [wf, setWf] = useState(initial);
  return (
    <div className="bg-surface-secondary p-8 min-w-[640px]">
      <ScheduleCard
        workflow={wf}
        onChange={setWf}
        submitLabel={submitLabel}
        onSubmit={() => {}}
      />
    </div>
  );
}

export const Daily9am: Story = {
  render: () => (
    <Harness
      submitLabel="Create Workflow"
      initial={{
        name: "demo",
        schedule: "0 9 * * *",
        timezone: "America/Los_Angeles",
        enabled: true,
        steps: [],
      }}
    />
  ),
};

export const Weekdays: Story = {
  render: () => (
    <Harness
      submitLabel="Save Changes"
      initial={{
        name: "demo",
        schedule: "30 8 * * 1-5",
        timezone: "Europe/London",
        enabled: true,
        steps: [],
      }}
    />
  ),
};

export const Custom: Story = {
  render: () => (
    <Harness
      submitLabel="Create Workflow"
      initial={{
        name: "",
        schedule: "*/15 0-6 1,15 * 1#2",
        timezone: "UTC",
        enabled: false,
        steps: [],
      }}
    />
  ),
};
