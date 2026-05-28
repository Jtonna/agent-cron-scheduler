import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { HeroDailyCostSlide } from "./HeroDailyCostSlide";
import type { DailyCostBucket } from "@/apis/types";

const meta: Meta<typeof HeroDailyCostSlide> = {
  title: "Components/Widgets/HeroDailyCostSlide",
  component: HeroDailyCostSlide,
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
type Story = StoryObj<typeof HeroDailyCostSlide>;

function makeSeries(values: number[]): DailyCostBucket[] {
  const today = new Date();
  return values.map((cost, i) => {
    const d = new Date(today);
    d.setDate(today.getDate() - (values.length - 1 - i));
    return {
      date: d.toISOString().slice(0, 10),
      runs_completed: cost > 0 ? 1 : 0,
      runs_failed: 0,
      runs_killed: 0,
      cost_from_completed: cost,
      cost_from_failed: 0,
      cost_from_killed: 0,
      total_usd: cost,
      tokens_in_from_completed: 0,
      tokens_in_from_failed: 0,
      tokens_in_from_killed: 0,
      tokens_out_from_completed: 0,
      tokens_out_from_failed: 0,
      tokens_out_from_killed: 0,
      total_input_tokens: 0,
      total_output_tokens: 0,
    };
  });
}

const VARIED = makeSeries([
  0.1, 0.2, 0.18, 0.4, 0.55, 0.32, 0.6, 0.44, 0.71, 0.88, 0.62, 0.41, 0.78, 1.1, 0.95, 1.32, 1.18,
  0.9, 0.76, 1.05, 1.32, 1.58, 1.4, 1.18, 1.92, 2.15, 1.84, 2.04, 2.31, 2.55,
]);

export const Populated: Story = { args: { data: VARIED, loading: false } };
export const Empty: Story = { args: { data: [], loading: false } };
export const Loading: Story = { args: { data: [], loading: true } };
