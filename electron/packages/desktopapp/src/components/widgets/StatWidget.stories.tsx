import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { DollarSign } from "lucide-react";
import { StatWidget } from "./StatWidget";

const meta: Meta<typeof StatWidget> = {
  title: "Components/Widgets/StatWidget",
  component: StatWidget,
};
export default meta;

type Story = StoryObj<typeof StatWidget>;

export const Default: Story = {
  args: {
    title: "Sample widget",
    children: <div className="text-fg text-sm">Body content goes here.</div>,
  },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};

export const WithIcon: Story = {
  args: {
    title: "Cost",
    icon: <DollarSign size={14} />,
    children: (
      <div className="flex flex-col gap-2 text-sm">
        <div className="flex justify-between">
          <span className="text-fg-muted">Today</span>
          <span className="font-mono text-fg">$0.42</span>
        </div>
        <div className="flex justify-between">
          <span className="text-fg-muted">Week</span>
          <span className="font-mono text-fg">$3.18</span>
        </div>
        <div className="flex justify-between">
          <span className="text-fg-muted">Month</span>
          <span className="font-mono text-fg">$12.04</span>
        </div>
      </div>
    ),
  },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};
