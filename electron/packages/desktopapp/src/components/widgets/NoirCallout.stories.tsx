import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { DollarSign } from "lucide-react";
import { NoirCallout } from "./NoirCallout";

const meta: Meta<typeof NoirCallout> = {
  title: "Components/Widgets/NoirCallout",
  component: NoirCallout,
  parameters: {
    layout: "centered",
  },
};
export default meta;
type Story = StoryObj<typeof NoirCallout>;

export const Default: Story = {
  args: {
    eyebrow: "This month",
    children: (
      <>
        <div className="text-display text-4xl num leading-none">$184.32</div>
        <div className="text-eyebrow mt-2">total spend</div>
      </>
    ),
  },
  decorators: [
    (Story) => (
      <div data-mesh="peach" className="rounded-[28px] p-6 w-[320px]">
        <Story />
      </div>
    ),
  ],
};

export const WithIcon: Story = {
  args: {
    eyebrow: "Today",
    icon: <DollarSign size={12} />,
    children: (
      <>
        <div className="text-display text-3xl num leading-none">$12.04</div>
        <div className="text-eyebrow mt-2">since midnight</div>
      </>
    ),
  },
  decorators: [
    (Story) => (
      <div data-mesh="mist" className="rounded-[28px] p-6 w-[320px]">
        <Story />
      </div>
    ),
  ],
};
