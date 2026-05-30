/**
 * KindSwitcherPopover stories — the popover triggered from the modal
 * header kind pill.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { KindSwitcherPopover } from "./KindSwitcherPopover";
import { makeDefaultStep, type NewShellStep } from "./types";

const meta: Meta<typeof KindSwitcherPopover> = {
  title: "Pages/CreateWorkflow/KindSwitcherPopover",
  component: KindSwitcherPopover,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof KindSwitcherPopover>;

export const FromDefault: Story = {
  render: () => (
    <div className="relative bg-surface-secondary p-8 h-[400px] w-[400px]">
      <KindSwitcherPopover
        current={makeDefaultStep("set_var")}
        onPick={() => {}}
        onClose={() => {}}
      />
    </div>
  ),
};

export const WithWarning: Story = {
  render: () => (
    <div className="relative bg-surface-secondary p-8 h-[400px] w-[400px]">
      <KindSwitcherPopover
        current={{
          ...(makeDefaultStep("shell") as NewShellStep),
          command: 'tar czf "/backups/db-${input.date}.tar.gz" /var/lib',
        }}
        onPick={() => {}}
        onClose={() => {}}
      />
    </div>
  ),
};
