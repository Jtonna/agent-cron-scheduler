import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobCostTrendWidget } from "./JobCostTrendWidget";
import type { JobDailyCostDataPoint } from "@/apis/types";

const meta: Meta<typeof JobCostTrendWidget> = {
  title: "Components/Widgets/JobCostTrendWidget",
  component: JobCostTrendWidget,
};
export default meta;

type Story = StoryObj<typeof JobCostTrendWidget>;

function makeSeries(values: number[]): JobDailyCostDataPoint[] {
  const today = new Date();
  return values.map((cost, i) => {
    const d = new Date(today);
    d.setDate(today.getDate() - (values.length - 1 - i));
    return {
      date: d.toISOString().slice(0, 10),
      runs: Math.max(1, Math.round(cost * 10)),
      cost,
      input_tokens: Math.round(cost * 50_000),
      output_tokens: Math.round(cost * 12_000),
    };
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
