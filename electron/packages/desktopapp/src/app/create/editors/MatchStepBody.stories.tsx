/**
 * MatchStepBody stories — empty + filled.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { MatchStepBody } from "./MatchStepBody";
import { makeDefaultStep, type NewMatchStep } from "../types";

const meta: Meta<typeof MatchStepBody> = {
  title: "Pages/CreateWorkflow/Editors/MatchStepBody",
  component: MatchStepBody,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof MatchStepBody>;

function Harness({ initial }: { initial: NewMatchStep }) {
  const [v, setV] = useState(initial);
  return (
    <div className="max-w-xl p-4 bg-surface-secondary border border-border rounded-menu">
      <MatchStepBody value={v} onChange={setV} onDrillIn={() => {}} />
    </div>
  );
}

export const Empty: Story = {
  render: () => <Harness initial={makeDefaultStep("match") as NewMatchStep} />,
};

export const Filled: Story = {
  render: () => (
    <Harness
      initial={{
        ...(makeDefaultStep("match") as NewMatchStep),
        id: "route_mood",
        expr: "${steps.classify.exports.mood}",
        cases: {
          hot: [makeDefaultStep("shell"), makeDefaultStep("shell"), makeDefaultStep("shell")],
          cold: [makeDefaultStep("shell"), makeDefaultStep("shell")],
        },
        default: [makeDefaultStep("shell")],
      }}
    />
  ),
};
