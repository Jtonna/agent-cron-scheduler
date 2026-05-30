/**
 * KindPaletteTray stories — left-edge dock of kind chips.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { KindPaletteTray } from "./KindPaletteTray";

const meta: Meta<typeof KindPaletteTray> = {
  title: "Pages/CreateWorkflow/KindPaletteTray",
  component: KindPaletteTray,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof KindPaletteTray>;

export const Default: Story = {
  render: () => (
    <div className="relative h-screen bg-surface-secondary">
      <KindPaletteTray onAdd={() => {}} />
    </div>
  ),
};
