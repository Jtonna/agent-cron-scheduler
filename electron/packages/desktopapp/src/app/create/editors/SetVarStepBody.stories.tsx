/**
 * SetVarStepBody stories — empty + filled.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SetVarStepBody } from "./SetVarStepBody";
import { makeDefaultStep, type NewSetVarStep } from "../types";

const meta: Meta<typeof SetVarStepBody> = {
  title: "Pages/CreateWorkflow/Editors/SetVarStepBody",
  component: SetVarStepBody,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof SetVarStepBody>;

function Harness({ initial }: { initial: NewSetVarStep }) {
  const [v, setV] = useState(initial);
  return (
    <div className="max-w-xl p-4 bg-surface-secondary border border-border rounded-menu">
      <SetVarStepBody value={v} onChange={setV} />
    </div>
  );
}

export const Empty: Story = {
  render: () => <Harness initial={{ ...(makeDefaultStep("set_var") as NewSetVarStep), exports: {} }} />,
};

export const Filled: Story = {
  render: () => (
    <Harness
      initial={{
        ...(makeDefaultStep("set_var") as NewSetVarStep),
        exports: {
          city: "${input.city}",
          lat: "${input.lat}",
          units: '"imperial"',
        },
      }}
    />
  ),
};
