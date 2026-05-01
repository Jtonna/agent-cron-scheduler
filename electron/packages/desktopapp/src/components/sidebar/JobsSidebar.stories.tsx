import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobsSidebar } from "./JobsSidebar";
import type { Job } from "@/apis/types";

const meta: Meta<typeof JobsSidebar> = {
  title: "Components/Sidebar/JobsSidebar",
  component: JobsSidebar,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof JobsSidebar>;

function makeJob(name: string, daysAgo: number): Job {
  const ts = new Date(Date.now() - daysAgo * 86400_000).toISOString();
  return {
    id: name,
    name,
    schedule: "0 0 * * *",
    execution: { type: "ShellCommand", value: "echo hi" },
    enabled: true,
    timezone: null,
    working_dir: null,
    env_vars: null,
    timeout_secs: 3600,
    log_environment: false,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: ts,
    updated_at: ts,
    last_run_at: ts,
    last_exit_code: 0,
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

export const Empty: Story = {
  args: { jobs: [] },
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
