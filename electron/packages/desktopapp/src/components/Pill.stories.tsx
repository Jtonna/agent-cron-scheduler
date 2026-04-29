import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Pill } from "./Pill";

const meta: Meta<typeof Pill> = {
  title: "Components/Pill",
  component: Pill,
};
export default meta;

type Story = StoryObj<typeof Pill>;

export const Default: Story = {
  args: { label: "backup-db" },
};

export const Active: Story = {
  args: { label: "backup-db", active: true },
};

export const Group: StoryObj = {
  render: () => (
    <div className="flex gap-2">
      <Pill label="backup-db" active />
      <Pill label="sync-users" />
      <Pill label="health-check" />
      <Pill label="deploy-staging" />
    </div>
  ),
};
