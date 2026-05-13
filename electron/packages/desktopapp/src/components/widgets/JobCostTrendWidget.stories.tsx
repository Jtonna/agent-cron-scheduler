import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobCostTrendWidget } from "./JobCostTrendWidget";
import type { DailyCostBucket } from "@/apis/types";

const meta: Meta<typeof JobCostTrendWidget> = {
  title: "Components/Widgets/JobCostTrendWidget",
  component: JobCostTrendWidget,
};
export default meta;

type Story = StoryObj<typeof JobCostTrendWidget>;

function makeBucket(date: string, total_usd: number): DailyCostBucket {
  return {
    date,
    runs_completed: Math.max(1, Math.round(total_usd * 10)),
    runs_failed: 0,
    runs_killed: 0,
    cost_from_completed: total_usd,
    cost_from_failed: 0,
    cost_from_killed: 0,
    total_usd,
    tokens_in_from_completed: Math.round(total_usd * 50_000),
    tokens_in_from_failed: 0,
    tokens_in_from_killed: 0,
    tokens_out_from_completed: Math.round(total_usd * 12_000),
    tokens_out_from_failed: 0,
    tokens_out_from_killed: 0,
    total_input_tokens: Math.round(total_usd * 50_000),
    total_output_tokens: Math.round(total_usd * 12_000),
  };
}

function makeSeries(values: number[]): DailyCostBucket[] {
  const today = new Date();
  return values.map((cost, i) => {
    const d = new Date(today);
    d.setDate(today.getDate() - (values.length - 1 - i));
    return makeBucket(d.toISOString().slice(0, 10), cost);
  });
}

const VARIED = makeSeries([
  0.05, 0.12, 0.09, 0.22, 0.31, 0.18, 0.34, 0.28, 0.41, 0.52, 0.36, 0.24, 0.45, 0.62, 0.55, 0.78,
  0.69, 0.51, 0.43, 0.6, 0.74, 0.91, 0.81, 0.66, 1.08, 1.22, 1.04, 1.18, 1.31, 1.45,
]);

const FLAT = makeSeries(new Array(30).fill(0.25));

export const Default: Story = {
  args: { data: VARIED },
  decorators: [
    (Story) => (
      <div style={{ width: 720, padding: 16 }}>
        <Story />
      </div>
    ),
  ],
};

export const Flat: Story = {
  args: { data: FLAT },
  decorators: [
    (Story) => (
      <div style={{ width: 720, padding: 16 }}>
        <Story />
      </div>
    ),
  ],
};

export const Empty: Story = {
  args: { data: [] },
  decorators: [
    (Story) => (
      <div style={{ width: 720, padding: 16 }}>
        <Story />
      </div>
    ),
  ],
};
