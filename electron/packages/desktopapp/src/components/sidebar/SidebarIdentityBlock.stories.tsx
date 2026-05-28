import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Star } from "lucide-react";
import { SidebarIdentityBlock } from "./SidebarIdentityBlock";

const meta: Meta<typeof SidebarIdentityBlock> = {
  title: "Components/Sidebar/SidebarIdentityBlock",
  component: SidebarIdentityBlock,
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
type Story = StoryObj<typeof SidebarIdentityBlock>;

export const WithCronAndStar: Story = {
  args: {
    title: "weather-shell-claude",
    meta: "0 * * * * · America/Los_Angeles",
    monoMeta: true,
    actions: (
      <button
        type="button"
        aria-label="Favorite"
        className="p-1.5 rounded-input hover:bg-surface-hover text-fg-subtle"
      >
        <Star size={16} />
      </button>
    ),
  },
};

export const RunIdentity: Story = {
  args: {
    title: "nightly-cleanup",
    meta: "0 2 * * *",
    monoMeta: true,
  },
};

export const LongName: Story = {
  args: {
    title: "a-rather-extraordinarily-long-workflow-name-that-must-truncate",
    meta: "0 * * * *",
    monoMeta: true,
  },
};
