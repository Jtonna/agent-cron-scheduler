/**
 * ScriptStepBody stories — empty + filled.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { ScriptStepBody } from "./ScriptStepBody";
import { makeDefaultStep, type NewScriptStep } from "../types";

const meta: Meta<typeof ScriptStepBody> = {
  title: "Pages/CreateWorkflow/Editors/ScriptStepBody",
  component: ScriptStepBody,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof ScriptStepBody>;

function Harness({ initial }: { initial: NewScriptStep }) {
  const [v, setV] = useState(initial);
  return (
    <div className="max-w-xl p-4 bg-surface-secondary border border-border rounded-menu">
      <ScriptStepBody value={v} onChange={setV} />
    </div>
  );
}

export const Empty: Story = {
  render: () => <Harness initial={makeDefaultStep("script") as NewScriptStep} />,
};

export const Filled: Story = {
  render: () => (
    <Harness
      initial={{
        ...(makeDefaultStep("script") as NewScriptStep),
        path: "./scripts/parse_response.py",
        script_type: "python",
        args: ["--input", "${steps.fetch_weather.stdout}", "--format", "json"],
        pass_stdin: true,
      }}
    />
  ),
};
