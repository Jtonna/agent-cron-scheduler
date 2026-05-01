import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarSection } from "./SidebarSection";

const meta: Meta<typeof SidebarSection> = {
  title: "Components/Sidebar/SidebarSection",
  component: SidebarSection,
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof SidebarSection>;

export const WithContent: Story = {
  args: {
    title: "Quick actions",
    children: <div className="px-2 py-2 text-sm text-fg-secondary">Section content goes here</div>,
  },
};

export const Empty: Story = {
  args: {
    title: "Favorited",
    emptyText: "No favorites yet",
  },
};
