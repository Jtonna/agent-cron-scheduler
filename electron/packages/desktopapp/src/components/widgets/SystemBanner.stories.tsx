import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SystemBanner } from "./SystemBanner";

const meta: Meta<typeof SystemBanner> = {
  title: "Components/Widgets/SystemBanner",
  component: SystemBanner,
  parameters: {
    layout: "padded",
  },
};
export default meta;

type Story = StoryObj<typeof SystemBanner>;

export const Default: Story = {};

export const UpdateAvailable: Story = {
  args: {
    updateAvailable: "3.2.0",
  },
};

export const BackupsDisabled: Story = {
  args: {
    backupsEnabled: false,
  },
};
