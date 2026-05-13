import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { SidebarRecentJobs } from "./SidebarRecentJobs";
import type { Job } from "@/apis/types";

const meta: Meta<typeof SidebarRecentJobs> = {
  title: "Components/Sidebar/SidebarRecentJobs",
  component: SidebarRecentJobs,
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof SidebarRecentJobs>;

function makeJob(name: string, daysAgo: number): Job {
  const ts = new Date(Date.now() - daysAgo * 86400_000).toISOString();
  return {
    id: name,
    name,
    schedule: "0 0 * * *",
    schedule_mode: "Cron",
    enabled: true,
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
  };
}

const SAMPLE: Job[] = [
  makeJob("backup-db", 0),
  makeJob("sync-users", 0.1),
  makeJob("health-check", 0.5),
  makeJob("cleanup-logs", 1),
  makeJob("nightly-report", 2),
];

export const Default: Story = {
  args: { jobs: SAMPLE },
};

export const Empty: Story = {
  args: { jobs: [] },
};
