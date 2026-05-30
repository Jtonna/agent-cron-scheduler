/**
 * WorkflowNameInline stories.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { WorkflowNameInline } from "./WorkflowNameInline";

const meta: Meta<typeof WorkflowNameInline> = {
  title: "Pages/CreateWorkflow/WorkflowNameInline",
  component: WorkflowNameInline,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof WorkflowNameInline>;

function Harness({ initial }: { initial: string }) {
  const [v, setV] = useState(initial);
  return (
    <div className="p-8 bg-surface-secondary">
      <WorkflowNameInline value={v} onChange={setV} />
    </div>
  );
}

export const Empty: Story = { render: () => <Harness initial="" /> };
export const Filled: Story = { render: () => <Harness initial="weather-greeter-demo" /> };
