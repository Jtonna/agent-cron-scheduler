import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarBackLink } from "./SidebarBackLink";

const meta: Meta<typeof SidebarBackLink> = {
  title: "Components/Sidebar/SidebarBackLink",
  component: SidebarBackLink,
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
type Story = StoryObj<typeof SidebarBackLink>;

export const Default: Story = {
  args: { href: "/workflows", children: "Back to Workflows" },
};

export const LongDestination: Story = {
  args: {
    href: "/workflows/x",
    children: "Back to a-rather-extraordinarily-long-workflow-name",
  },
};
