import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SystemHeroCarousel } from "./SystemHeroCarousel";

const meta: Meta<typeof SystemHeroCarousel> = {
  title: "Components/Widgets/SystemHeroCarousel",
  component: SystemHeroCarousel,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;
type Story = StoryObj<typeof SystemHeroCarousel>;

/**
 * Default story. Renders the carousel against a Storybook QueryClient
 * that returns loading states for both `useGlobalCostSummary` and
 * `useJobs`, so all slides render their skeleton/empty branches. The
 * 5-second auto-rotation, hover-to-pause, and dot navigation are all
 * exercised here.
 */
export const Default: Story = { args: {} };

/**
 * Shorter hero — useful for testing the carousel embedded inside a
 * smaller card surface.
 */
export const Compact: Story = { args: { minHeight: 280 } };
