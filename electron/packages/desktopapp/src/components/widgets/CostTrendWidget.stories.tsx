import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { CostTrendWidget } from "./CostTrendWidget";
import type { GlobalDailyTrend } from "@/apis/types";

const meta: Meta<typeof CostTrendWidget> = {
  title: "Components/Widgets/CostTrendWidget",
  component: CostTrendWidget,
};
export default meta;

type Story = StoryObj<typeof CostTrendWidget>;

function makeSeries(values: number[]): GlobalDailyTrend[] {
  const today = new Date();
  return values.map((cost, i) => {
    const d = new Date(today);
    d.setDate(today.getDate() - (values.length - 1 - i));
    return {
      date: d.toISOString().slice(0, 10),
      cost_usd: cost,
      input_tokens: cost * 50_000,
      output_tokens: cost * 12_000,
    };
  });
}

const VARIED = makeSeries([
  0.1, 0.2, 0.18, 0.4, 0.55, 0.32, 0.6, 0.44, 0.71, 0.88, 0.62, 0.41, 0.78, 1.1, 0.95, 1.32, 1.18,
  0.9, 0.76, 1.05, 1.32, 1.58, 1.4, 1.18, 1.92, 2.15, 1.84, 2.04, 2.31, 2.55,
]);

const FLAT = makeSeries(new Array(30).fill(0.5));

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
