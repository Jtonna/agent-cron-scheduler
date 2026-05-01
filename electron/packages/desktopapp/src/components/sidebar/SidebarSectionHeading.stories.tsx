import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarSectionHeading } from "./SidebarSectionHeading";

const meta: Meta<typeof SidebarSectionHeading> = {
  title: "Components/Sidebar/SidebarSectionHeading",
  component: SidebarSectionHeading,
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof SidebarSectionHeading>;

export const Default: Story = {
  args: {
    children: "Quick actions",
  },
};
