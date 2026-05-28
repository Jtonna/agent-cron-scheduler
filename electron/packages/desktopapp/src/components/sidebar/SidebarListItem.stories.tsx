import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarListItem } from "./SidebarListItem";

const meta: Meta<typeof SidebarListItem> = {
  title: "Components/Sidebar/SidebarListItem",
  component: SidebarListItem,
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
type Story = StoryObj<typeof SidebarListItem>;

export const RunRow: Story = {
  args: {
    state: "success",
    title: "#a1b2c3d4",
    meta: "2m ago",
    href: "/workflows/x/runs/a1b2c3d4",
  },
};

export const StepRowActive: Story = {
  args: {
    state: "success",
    title: "summarize-with-claude",
    meta: "2m",
    metaSecondary: "$0.034",
    active: true,
    onPress: () => {},
  },
};

export const Failed: Story = {
  args: {
    state: "failed",
    title: "#deadbeef",
    meta: "1h ago",
    href: "/workflows/x/runs/deadbeef",
  },
};

export const Running: Story = {
  args: {
    state: "running",
    title: "post-to-slack",
    meta: "—",
    metaSecondary: "—",
    onPress: () => {},
  },
};
