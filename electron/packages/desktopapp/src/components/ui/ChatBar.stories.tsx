import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { ChatBar } from "./ChatBar";

const meta: Meta<typeof ChatBar> = {
  title: "Components/UI/ChatBar",
  component: ChatBar,
  parameters: {
    layout: "padded",
  },
};
export default meta;

type Story = StoryObj<typeof ChatBar>;

export const Default: Story = {};

export const CustomPlaceholder: Story = {
  args: {
    placeholder: "Describe the job you want to create...",
  },
};
