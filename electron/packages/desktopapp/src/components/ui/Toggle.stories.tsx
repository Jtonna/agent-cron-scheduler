import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { useState } from "react";
import { Toggle } from "./Toggle";

const meta: Meta<typeof Toggle> = {
  title: "Components/UI/Toggle",
  component: Toggle,
  parameters: { layout: "centered" },
};
export default meta;
type Story = StoryObj<typeof Toggle>;

export const Off: Story = {
  args: { checked: false, ariaLabel: "Demo toggle", onChange: () => {} },
};

export const On: Story = {
  args: { checked: true, ariaLabel: "Demo toggle", onChange: () => {} },
};

export const Disabled: Story = {
  args: { checked: true, ariaLabel: "Demo toggle", disabled: true, onChange: () => {} },
};

export const Interactive: Story = {
  render: () => {
    const [on, setOn] = useState(false);
    return <Toggle checked={on} onChange={setOn} ariaLabel="Interactive demo toggle" />;
  },
};

export const Medium: Story = {
  args: { checked: true, size: "md", ariaLabel: "Demo toggle", onChange: () => {} },
};
