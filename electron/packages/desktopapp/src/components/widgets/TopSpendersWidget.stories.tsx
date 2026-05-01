import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { TopSpendersWidget } from "./TopSpendersWidget";
import type { GlobalTopJob } from "@/apis/types";

const meta: Meta<typeof TopSpendersWidget> = {
  title: "Components/Widgets/TopSpendersWidget",
  component: TopSpendersWidget,
};
export default meta;

type Story = StoryObj<typeof TopSpendersWidget>;

const MOCK: GlobalTopJob[] = [
  { job_id: "1", job_name: "nightly-report", total_cost: 4.2, total_runs: 14 },
  { job_id: "2", job_name: "sync-users", total_cost: 2.18, total_runs: 87 },
  { job_id: "3", job_name: "cleanup-logs", total_cost: 1.04, total_runs: 30 },
  { job_id: "4", job_name: "backup-db", total_cost: 0.74, total_runs: 21 },
  { job_id: "5", job_name: "deploy-staging", total_cost: 0.31, total_runs: 5 },
];

export const Default: Story = {
  args: { jobs: MOCK },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 360 }}>
        <Story />
      </div>
    ),
  ],
};

export const Empty: Story = {
  args: { jobs: [] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 360 }}>
        <Story />
      </div>
    ),
  ],
};
