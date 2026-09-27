import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { DeleteJobDialog } from "./DeleteJobDialog";

const meta: Meta<typeof DeleteJobDialog> = {
  title: "Components/Sidebar/DeleteJobDialog",
  component: DeleteJobDialog,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof DeleteJobDialog>;

export const Default: Story = {
  args: {
    isOpen: true,
    jobName: "backup-db",
    onOpenChange: () => {},
    onConfirm: () => {},
  },
};

export const Closed: Story = {
  args: {
    isOpen: false,
    jobName: "backup-db",
    onOpenChange: () => {},
    onConfirm: () => {},
  },
};

export const Pending: Story = {
  args: {
    isOpen: true,
    jobName: "backup-db",
    onOpenChange: () => {},
    onConfirm: () => new Promise(() => {}),
    isPending: true,
  },
};

export const ConflictError: Story = {
  args: {
    isOpen: true,
    jobName: "weather-daily",
    onOpenChange: () => {},
    onConfirm: () => {},
    error:
      "Cannot delete workflow 'weather-daily' while run 01a0e215-… is still running. Kill the run or wait for it to finish, then retry.",
  },
};
