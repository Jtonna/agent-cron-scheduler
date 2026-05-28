import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { HeroTotalCostSlide } from "./HeroTotalCostSlide";
import type { WorkflowCostSummary } from "@/apis/types";

const meta: Meta<typeof HeroTotalCostSlide> = {
  title: "Components/Widgets/HeroTotalCostSlide",
  component: HeroTotalCostSlide,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div style={{ width: 600, height: 420 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;
type Story = StoryObj<typeof HeroTotalCostSlide>;

const MOCK: WorkflowCostSummary = {
  computed_at: new Date().toISOString(),
  last_30_days_runs: 248,
  last_30_days_total_usd: 312.47,
  last_year_runs: 3140,
  last_year_total_usd: 4280.16,
  last_30_days_input_tokens: 0,
  last_30_days_output_tokens: 0,
  last_year_input_tokens: 0,
  last_year_output_tokens: 0,
  daily_buckets: [],
};

export const Populated: Story = { args: { summary: MOCK, loading: false } };
export const Empty: Story = {
  args: {
    summary: { ...MOCK, last_30_days_runs: 0, last_30_days_total_usd: 0 },
    loading: false,
  },
};
export const Loading: Story = { args: { summary: null, loading: true } };
