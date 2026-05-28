import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarSectionHeader } from "./SidebarSectionHeader";

const meta: Meta<typeof SidebarSectionHeader> = {
  title: "Components/Sidebar/SidebarSectionHeader",
  component: SidebarSectionHeader,
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
type Story = StoryObj<typeof SidebarSectionHeader>;

export const Plain: Story = { args: { title: "Status" } };
export const WithCount: Story = { args: { title: "Recent Runs", meta: "(6)" } };
export const WithSuffix: Story = {
  args: { title: "Steps", meta: "5 of 7 ran" },
};
