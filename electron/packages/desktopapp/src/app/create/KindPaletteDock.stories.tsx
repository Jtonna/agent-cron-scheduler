/**
 * KindPaletteDock stories — bottom-centre horizontal dock of kind chips.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { KindPaletteDock } from "./KindPaletteDock";

const meta: Meta<typeof KindPaletteDock> = {
  title: "Pages/CreateWorkflow/KindPaletteDock",
  component: KindPaletteDock,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof KindPaletteDock>;

export const Default: Story = {
  render: () => (
    <div className="relative h-screen bg-surface-tertiary">
      <KindPaletteDock onAdd={() => {}} />
    </div>
  ),
};
