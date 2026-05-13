import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { HealthWidget, type RunStatus } from "./HealthWidget";

const meta: Meta<typeof HealthWidget> = {
  title: "Components/Widgets/HealthWidget",
  component: HealthWidget,
};
export default meta;

type Story = StoryObj<typeof HealthWidget>;

interface FixtureRun {
  started_at: string;
  status: RunStatus;
}

function makeRun(status: RunStatus, daysAgo: number = 0): FixtureRun {
  const started = new Date(Date.now() - daysAgo * 86400_000).toISOString();
  return { started_at: started, status };
}

const MIXED: FixtureRun[] = [
  ...Array.from({ length: 12 }, (_, i) => makeRun("Completed", i % 13)),
  ...Array.from({ length: 3 }, (_, i) => makeRun("Failed", i % 13)),
  ...Array.from({ length: 1 }, (_, i) => makeRun("Running", i % 13)),
  ...Array.from({ length: 1 }, (_, i) => makeRun("CompletedWithWarnings", i % 13)),
];

export const Default: Story = {
  args: { runs: MIXED },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};

export const AllSuccess: Story = {
  args: {
    runs: Array.from({ length: 8 }, (_, i) => makeRun("Completed", i)),
  },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};

export const NoRuns: Story = {
  args: { runs: [] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 320 }}>
        <Story />
      </div>
    ),
  ],
};
