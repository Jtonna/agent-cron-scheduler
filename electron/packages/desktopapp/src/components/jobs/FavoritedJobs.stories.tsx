import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { FavoritedJobs } from "./FavoritedJobs";

const meta: Meta<typeof FavoritedJobs> = {
  title: "Components/Workflows/FavoritedJobs",
  component: FavoritedJobs,
};
export default meta;

type Story = StoryObj<typeof FavoritedJobs>;

export const WithJobs: Story = {
  args: {
    jobs: [
      { id: "01941111-1111-7111-8111-111111111111", name: "backup-db" },
      { id: "01942222-2222-7222-8222-222222222222", name: "sync-users" },
      { id: "01943333-3333-7333-8333-333333333333", name: "health-check" },
      { id: "01944444-4444-7444-8444-444444444444", name: "deploy-staging" },
      { id: "01945555-5555-7555-8555-555555555555", name: "cleanup-logs" },
      { id: "01946666-6666-7666-8666-666666666666", name: "nightly-report" },
    ],
  },
};

export const Empty: Story = {
  args: {
    jobs: [],
  },
};
