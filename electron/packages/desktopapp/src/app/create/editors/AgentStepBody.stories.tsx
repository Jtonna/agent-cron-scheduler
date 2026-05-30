/**
 * AgentStepBody stories — empty + filled.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { AgentStepBody } from "./AgentStepBody";
import { makeDefaultStep, type NewAgentStep } from "../types";

const meta: Meta<typeof AgentStepBody> = {
  title: "Pages/CreateWorkflow/Editors/AgentStepBody",
  component: AgentStepBody,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof AgentStepBody>;

function Harness({ initial }: { initial: NewAgentStep }) {
  const [v, setV] = useState(initial);
  return (
    <div className="max-w-xl p-4 bg-surface-secondary border border-border rounded-menu">
      <AgentStepBody value={v} onChange={setV} />
    </div>
  );
}

export const Empty: Story = {
  render: () => (
    <Harness initial={{ ...(makeDefaultStep("agent") as NewAgentStep), prompt: "" }} />
  ),
};

export const Filled: Story = {
  render: () => (
    <Harness
      initial={{
        ...(makeDefaultStep("agent") as NewAgentStep),
        model: "claude-opus-4",
        extra_args: ["--max-turns", "6", "--no-tools"],
        prompt:
          "You are a concise summarizer.\n\nGiven the weather forecast in ${steps.fetch_weather.stdout},\nproduce a one-paragraph briefing for ${input.city}.\n\nKeep it under 280 characters. End with a single emoji.",
      }}
    />
  ),
};
