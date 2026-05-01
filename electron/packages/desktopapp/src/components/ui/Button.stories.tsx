import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Plus, Sparkles } from "lucide-react";
import { Button } from "./Button";

const meta: Meta<typeof Button> = {
  title: "Components/UI/Button",
  component: Button,
};
export default meta;

type Story = StoryObj<typeof Button>;

export const Primary: Story = {
  render: () => <Button>Build a Job</Button>,
  name: "Primary (default)",
};

export const Secondary: Story = {
  render: () => <Button intent="secondary">Cancel</Button>,
};

export const Ghost: Story = {
  render: () => <Button intent="ghost">Dismiss</Button>,
};

export const WithIcon: Story = {
  render: () => <Button icon={<Sparkles size={14} />}>Build a Job</Button>,
};

export const Sizes: Story = {
  render: () => (
    <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
      <Button size="sm">Small</Button>
      <Button size="md">Medium</Button>
      <Button size="lg">Large</Button>
    </div>
  ),
};

export const FullWidth: Story = {
  render: () => (
    <div style={{ width: 280, padding: 12 }}>
      <Button icon={<Plus size={14} />} fullWidth size="sm">
        New job
      </Button>
    </div>
  ),
};

export const Rounded: Story = {
  render: () => (
    <Button shape="rounded" icon={<Plus size={14} />}>
      Add item
    </Button>
  ),
};

export const AsLink: Story = {
  render: () => (
    <Button href="/create" icon={<Sparkles size={14} />}>
      Build a Job
    </Button>
  ),
};

export const AsAction: Story = {
  render: () => (
    <Button onPress={() => console.log("pressed")} icon={<Plus size={14} />}>
      Run action
    </Button>
  ),
};
