/**
 * HoverActions stories — small action row that floats above a step
 * node on hover. Rendered standalone here so the toolbar can be seen
 * without a reactflow canvas around it.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Terminal } from "lucide-react";
import { HoverActions } from "./HoverActions";

const meta: Meta<typeof HoverActions> = {
  title: "Pages/CreateWorkflow/HoverActions",
  component: HoverActions,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof HoverActions>;

export const Default: Story = {
  render: () => (
    <div className="relative group bg-surface border border-border rounded-card w-[240px] h-[80px] flex items-center justify-center text-fg-muted text-sm">
      hover me
      <HoverActions
        KindIcon={Terminal}
        onSwitchKind={() => {}}
        onEdit={() => {}}
        onDelete={() => {}}
        canDelete
      />
    </div>
  ),
};

export const Undeletable: Story = {
  render: () => (
    <div className="relative group bg-surface border border-border rounded-card w-[240px] h-[80px] flex items-center justify-center text-fg-muted text-sm">
      hover me (delete disabled)
      <HoverActions
        KindIcon={Terminal}
        onSwitchKind={() => {}}
        onEdit={() => {}}
        onDelete={() => {}}
        canDelete={false}
      />
    </div>
  ),
};
