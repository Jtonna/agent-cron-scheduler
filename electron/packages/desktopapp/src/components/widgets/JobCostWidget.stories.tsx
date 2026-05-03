import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobCostWidget } from "./JobCostWidget";
import type { JobCostSummaryResponse } from "@/apis/types";

const meta: Meta<typeof JobCostWidget> = {
  title: "Components/Widgets/JobCostWidget",
  component: JobCostWidget,
};
export default meta;

type Story = StoryObj<typeof JobCostWidget>;

const MOCK_SUMMARY: JobCostSummaryResponse = {
  job_id: "job_demo",
  timeframe: "30d",
  summary: {
    total_runs: 142,
    total_cost_usd: 8.74,
    avg_cost_per_run: 0.0615,
    total_duration_ms: 1_842_000,
    total_input_tokens: 1_250_000,
    total_output_tokens: 320_000,
    total_cache_read_tokens: 450_000,
    runs_by_status: { Completed: 138, Failed: 4 },
  },
  data: [],
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
