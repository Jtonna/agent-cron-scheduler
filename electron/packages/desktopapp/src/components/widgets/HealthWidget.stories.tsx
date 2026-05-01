import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { HealthWidget } from "./HealthWidget";
import type { RecentRunEntry } from "@/apis/types";

const meta: Meta<typeof HealthWidget> = {
  title: "Components/Widgets/HealthWidget",
  component: HealthWidget,
};
export default meta;

type Story = StoryObj<typeof HealthWidget>;

function makeRun(
  overrides: Partial<RecentRunEntry> & {
    status: RecentRunEntry["status"];
  },
  daysAgo: number = 0,
): RecentRunEntry {
  const started = new Date(Date.now() - daysAgo * 86400_000).toISOString();
  return {
    run_id: Math.random().toString(36).slice(2),
    job_id: "job-1",
    job_name: "demo",
    started_at: started,
    finished_at: started,
    exit_code: 0,
    log_size_bytes: 0,
    error: null,
    ...overrides,
  };
}

const MIXED: RecentRunEntry[] = [
  ...Array.from({ length: 12 }, (_, i) => makeRun({ status: "Completed" }, i % 13)),
  ...Array.from({ length: 3 }, (_, i) => makeRun({ status: "Failed" }, i % 13)),
  ...Array.from({ length: 1 }, (_, i) => makeRun({ status: "Running" }, i % 13)),
  ...Array.from({ length: 1 }, (_, i) => makeRun({ status: "CompletedWithWarnings" }, i % 13)),
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
    runs: Array.from({ length: 8 }, (_, i) => makeRun({ status: "Completed" }, i)),
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
