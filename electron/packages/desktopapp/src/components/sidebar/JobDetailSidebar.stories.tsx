import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobDetailSidebar } from "./JobDetailSidebar";
import type { Job } from "@/apis/types";

const meta: Meta<typeof JobDetailSidebar> = {
  title: "Components/Sidebar/JobDetailSidebar",
  component: JobDetailSidebar,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <div
        style={{ width: 280, borderRight: "1px solid var(--color-border-subtle)", height: "100vh" }}
      >
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof JobDetailSidebar>;

function makeJob(name: string, daysAgo: number, overrides: Partial<Job> = {}): Job {
  const ts = new Date(Date.now() - daysAgo * 86400_000).toISOString();
  return {
    id: name,
    name,
    schedule: "0 2 * * *",
    schedule_mode: "Cron",
    enabled: true,
    is_favorited: false,
    allow_concurrent: false,
    on_failure: "abort",
    steps: [],
    timezone: "UTC",
    working_dir: ".",
    env_vars: null,
    default_input: null,
    created_at: ts,
    updated_at: ts,
    version: 1,
    last_run_at: ts,
    last_run_id: null,
    last_run_status: "Completed",
    next_run_at: null,
    ...overrides,
  };
}

const CURRENT = makeJob("backup-db", 0);

export const Default: Story = {
  args: { job: CURRENT, runningCount: 0, favorited: false },
};

export const Running: Story = {
  args: { job: CURRENT, runningCount: 1, favorited: false },
};

export const MultipleRunning: Story = {
  args: { job: CURRENT, runningCount: 3, favorited: false },
};

export const Disabled: Story = {
  args: {
    job: makeJob("backup-db", 0, { enabled: false }),
    runningCount: 0,
    favorited: false,
  },
};

export const Favorited: Story = {
  args: { job: CURRENT, runningCount: 0, favorited: true },
};
