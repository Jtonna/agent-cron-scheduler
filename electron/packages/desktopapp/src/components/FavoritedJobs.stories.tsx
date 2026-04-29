import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { FavoritedJobs } from "./FavoritedJobs";

const meta: Meta<typeof FavoritedJobs> = {
  title: "Components/FavoritedJobs",
  component: FavoritedJobs,
};
export default meta;

type Story = StoryObj<typeof FavoritedJobs>;

export const WithJobs: Story = {
  args: {
    jobs: ["backup-db", "sync-users", "health-check", "deploy-staging", "cleanup-logs", "nightly-report"],
  },
};

export const Empty: Story = {
  args: {
    jobs: [],
  },
};
