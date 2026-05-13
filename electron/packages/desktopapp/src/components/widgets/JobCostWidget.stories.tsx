import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobCostWidget } from "./JobCostWidget";
import type { WorkflowCostEntry } from "@/apis/types";

const meta: Meta<typeof JobCostWidget> = {
  title: "Components/Widgets/JobCostWidget",
  component: JobCostWidget,
};
export default meta;

type Story = StoryObj<typeof JobCostWidget>;

const MOCK_SUMMARY: WorkflowCostEntry = {
  workflow_id: "wf_demo",
  workflow_name: "demo",
  cost_summary: {
    computed_at: new Date().toISOString(),
    last_30_days_runs: 142,
    last_30_days_total_usd: 8.74,
    last_year_runs: 1820,
    last_year_total_usd: 102.5,
    last_30_days_input_tokens: 1_250_000,
    last_30_days_output_tokens: 320_000,
    last_year_input_tokens: 15_000_000,
    last_year_output_tokens: 3_800_000,
    daily_buckets: [],
  },
};

export const Default: Story = {
  args: { summary: MOCK_SUMMARY, loading: false },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};

export const Loading: Story = {
  args: { summary: null, loading: true },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};
