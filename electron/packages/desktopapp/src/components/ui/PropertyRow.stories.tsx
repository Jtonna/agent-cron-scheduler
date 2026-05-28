import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { PropertyRow } from "./PropertyRow";
import { Toggle } from "./Toggle";

const meta: Meta<typeof PropertyRow> = {
  title: "Components/UI/PropertyRow",
  component: PropertyRow,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div style={{ width: 260 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;
type Story = StoryObj<typeof PropertyRow>;

export const Default: Story = {
  args: { label: "Timezone", value: "America/Los_Angeles" },
};

export const Mono: Story = {
  args: { label: "Schedule", value: "0 * * * *", mono: true },
};

export const Empty: Story = {
  args: { label: "Next run", value: undefined },
};

export const WithToggle: Story = {
  args: {
    label: "Enabled",
    value: <Toggle checked onChange={() => {}} ariaLabel="Toggle enabled" />,
  },
};

export const Stack: Story = {
  render: () => (
    <div className="flex flex-col">
      <PropertyRow label="Enabled" value={<Toggle checked onChange={() => {}} ariaLabel="Toggle enabled" />} />
      <PropertyRow label="Schedule" value="0 * * * *" mono />
      <PropertyRow label="Timezone" value="America/Los_Angeles" />
      <PropertyRow label="Next run" />
      <PropertyRow label="Last run" value="2m ago" />
    </div>
  ),
};
