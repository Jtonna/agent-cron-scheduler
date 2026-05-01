import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { TabBar } from "./TabBar";

const meta: Meta<typeof TabBar> = {
  title: "Components/UI/TabBar",
  component: TabBar,
  parameters: {
    layout: "fullscreen",
  },
};
export default meta;

type Story = StoryObj<typeof TabBar>;

export const Default: Story = {
  args: {
    label: "Recent",
    tabs: ["All runs", "Running", "Succeeded", "Failed"],
    activeTab: "All runs",
  },
};

export const WithActiveTab: Story = {
  args: {
    label: "Recent",
    tabs: ["All runs", "Running", "Succeeded", "Failed"],
    activeTab: "Running",
  },
};

export const NoLabel: Story = {
  args: {
    tabs: ["All runs", "Running", "Succeeded", "Failed"],
    activeTab: "All runs",
  },
};

export const NoFilter: Story = {
  args: {
    tabs: ["Overview", "Settings", "Logs"],
    activeTab: "Overview",
    showFilter: false,
  },
};
