import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Navbar, NavButton } from "./Navbar";
import { Bot, Plus } from "lucide-react";

const meta: Meta<typeof Navbar> = {
  title: "Components/Navbar",
  component: Navbar,
  parameters: {
    layout: "fullscreen",
  },
};
export default meta;

type Story = StoryObj<typeof Navbar>;

export const Default: Story = {};

export const NavButtonDefault: StoryObj<typeof NavButton> = {
  render: () => <NavButton>Build With AI</NavButton>,
  name: "NavButton — no icon",
};

export const NavButtonWithIcon: StoryObj<typeof NavButton> = {
  render: () => (
    <NavButton icon={<Bot size={14} />}>Build With AI</NavButton>
  ),
  name: "NavButton — with icon",
};

export const NavButtonCustom: StoryObj<typeof NavButton> = {
  render: () => (
    <NavButton icon={<Plus size={14} />} className="bg-gray-900 hover:bg-gray-800">
      New Job
    </NavButton>
  ),
  name: "NavButton — custom style",
};
