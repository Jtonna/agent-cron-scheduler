import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { CostWidget } from "./CostWidget";
import type { GlobalCostSummaryResponse } from "@/apis/types";

const meta: Meta<typeof CostWidget> = {
  title: "Components/Widgets/CostWidget",
  component: CostWidget,
};
export default meta;

type Story = StoryObj<typeof CostWidget>;

const MOCK_SUMMARY: GlobalCostSummaryResponse = {
  timeframe: "30d",
  today_usd: 0.42,
  week_usd: 3.18,
  month_usd: 12.04,
  today_tokens: { input: 12_500, output: 4_200 },
  top_jobs: [],
  daily_trend: [],
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
