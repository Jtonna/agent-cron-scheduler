import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { TokensWidget } from "./TokensWidget";
import type { WorkflowCostSummary } from "@/apis/types";

const meta: Meta<typeof TokensWidget> = {
  title: "Components/Widgets/TokensWidget",
  component: TokensWidget,
};
export default meta;

type Story = StoryObj<typeof TokensWidget>;

const todayKey = new Date().toISOString().slice(0, 10);
const yesterdayKey = (() => {
  const d = new Date();
  d.setDate(d.getDate() - 1);
  return d.toISOString().slice(0, 10);
})();

const MOCK_SUMMARY: WorkflowCostSummary = {
  computed_at: new Date().toISOString(),
  last_30_days_runs: 42,
  last_30_days_total_usd: 12.04,
  last_30_days_input_tokens: 1200,
  last_30_days_output_tokens: 31000,
  last_year_runs: 312,
  last_year_total_usd: 86.41,
  last_year_input_tokens: 15400,
  last_year_output_tokens: 412000,
  daily_buckets: [
    {
      date: yesterdayKey,
      runs_completed: 3,
      runs_failed: 0,
      runs_killed: 0,
      cost_from_completed: 2.76,
      cost_from_failed: 0,
      cost_from_killed: 0,
      total_usd: 2.76,
      tokens_in_from_completed: 320,
      tokens_in_from_failed: 0,
      tokens_in_from_killed: 0,
      tokens_out_from_completed: 8400,
      tokens_out_from_failed: 0,
      tokens_out_from_killed: 0,
      total_input_tokens: 320,
      total_output_tokens: 8400,
    },
    {
      date: todayKey,
      runs_completed: 2,
      runs_failed: 0,
      runs_killed: 0,
      cost_from_completed: 0.42,
      cost_from_failed: 0,
      cost_from_killed: 0,
      total_usd: 0.42,
      tokens_in_from_completed: 8,
      tokens_in_from_failed: 0,
      tokens_in_from_killed: 0,
      tokens_out_from_completed: 224,
      tokens_out_from_failed: 0,
      tokens_out_from_killed: 0,
      total_input_tokens: 8,
      total_output_tokens: 224,
    },
  ],
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
