import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Pencil, Play, Plus, Trash2 } from "lucide-react";
import { CompactActionButton } from "./CompactActionButton";

const meta: Meta<typeof CompactActionButton> = {
  title: "Components/UI/CompactActionButton",
  component: CompactActionButton,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div style={{ width: 260 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;
type Story = StoryObj<typeof CompactActionButton>;

export const Primary: Story = {
  args: {
    intent: "primary",
    icon: <Play size={12} />,
    children: "Run Workflow",
    fullWidth: true,
    onPress: () => {},
  },
};

export const Secondary: Story = {
  args: {
    intent: "secondary",
    icon: <Pencil size={12} />,
    children: "Edit Workflow",
    fullWidth: true,
    onPress: () => {},
  },
};

export const Ghost: Story = {
  args: {
    intent: "ghost",
    icon: <Pencil size={12} />,
    children: "Edit Workflow",
    fullWidth: true,
    onPress: () => {},
  },
};

export const Destructive: Story = {
  args: {
    intent: "destructive",
    icon: <Trash2 size={12} />,
    children: "Delete",
    fullWidth: true,
    onPress: () => {},
  },
};

export const AsLink: Story = {
  args: {
    intent: "primary",
    icon: <Plus size={12} />,
    children: "New Workflow",
    fullWidth: true,
    href: "/create",
  },
};

export const Row: Story = {
  render: () => (
    <div className="flex gap-2">
      <CompactActionButton intent="ghost" fullWidth icon={<Pencil size={12} />} onPress={() => {}}>
        Edit Workflow
      </CompactActionButton>
      <CompactActionButton intent="destructive" fullWidth icon={<Trash2 size={12} />} onPress={() => {}}>
        Delete
      </CompactActionButton>
    </div>
  ),
};

/**
 * DeleteIconLeftEditRight — the JobDetailSidebar secondary action row.
 * Delete is icon-only (w-9, left, destructive chrome), Edit fills the rest
 * (secondary chrome). Both render visible chrome so they read as siblings
 * of the Run split-button above them in the sidebar.
 */
export const DeleteIconLeftEditRight: Story = {
  render: () => (
    <div className="flex gap-2">
      <CompactActionButton
        intent="destructive"
        className="w-9 px-0 shrink-0"
        icon={<Trash2 size={12} />}
        onPress={() => {}}
        ariaLabel="Delete workflow"
      >
        {null}
      </CompactActionButton>
      <CompactActionButton intent="secondary" fullWidth icon={<Pencil size={12} />} onPress={() => {}}>
        Edit
      </CompactActionButton>
    </div>
  ),
};
