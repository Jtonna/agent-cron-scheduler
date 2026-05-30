/**
 * ShellStepBody stories — empty + filled.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { ShellStepBody } from "./ShellStepBody";
import { makeDefaultStep, type NewShellStep } from "../types";

const meta: Meta<typeof ShellStepBody> = {
  title: "Pages/CreateWorkflow/Editors/ShellStepBody",
  component: ShellStepBody,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof ShellStepBody>;

function Harness({ initial }: { initial: NewShellStep }) {
  const [v, setV] = useState(initial);
  return (
    <div className="max-w-xl p-4 bg-surface-secondary border border-border rounded-menu">
      <ShellStepBody value={v} onChange={setV} />
    </div>
  );
}

export const Empty: Story = {
  render: () => (
    <Harness
      initial={{ ...(makeDefaultStep("shell") as NewShellStep), command: "" }}
    />
  ),
};

export const Filled: Story = {
  render: () => (
    <Harness
      initial={{
        ...(makeDefaultStep("shell") as NewShellStep),
        command:
          'tar czf "/backups/db-${input.date}.tar.gz" \\\n  --exclude=\'*.log\' \\\n  /var/lib/${input.service}\n\necho "saved by ${steps.prev.exports.user}"',
        pass_stdin: true,
      }}
    />
  ),
};
