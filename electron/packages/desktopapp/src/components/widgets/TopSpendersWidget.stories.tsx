import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { TopSpendersWidget } from "./TopSpendersWidget";
import type { WorkflowCostEntry } from "@/apis/types";

const meta: Meta<typeof TopSpendersWidget> = {
  title: "Components/Widgets/TopSpendersWidget",
  component: TopSpendersWidget,
};
export default meta;

type Story = StoryObj<typeof TopSpendersWidget>;

function entry(id: string, name: string, cost: number, runs: number): WorkflowCostEntry {
  return {
    workflow_id: id,
    workflow_name: name,
    cost_summary: {
      computed_at: new Date().toISOString(),
      last_30_days_runs: runs,
      last_30_days_total_usd: cost,
      last_year_runs: runs,
      last_year_total_usd: cost,
      last_30_days_input_tokens: 0,
      last_30_days_output_tokens: 0,
      last_year_input_tokens: 0,
      last_year_output_tokens: 0,
      daily_buckets: [],
    },
  };
}

const MOCK: WorkflowCostEntry[] = [
  entry("1", "nightly-report", 4.2, 14),
  entry("2", "sync-users", 2.18, 87),
  entry("3", "cleanup-logs", 1.04, 30),
  entry("4", "backup-db", 0.74, 21),
  entry("5", "deploy-staging", 0.31, 5),
];

export const Default: Story = {
  args: { workflows: MOCK },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 360 }}>
        <Story />
      </div>
    ),
  ],
};

export const Empty: Story = {
  args: { workflows: [] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 360 }}>
        <Story />
      </div>
    ),
  ],
};
