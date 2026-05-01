import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobsList } from "./JobsList";
import type { Job } from "@/apis/types";

const meta: Meta<typeof JobsList> = {
  title: "Components/Jobs/JobsList",
  component: JobsList,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof JobsList>;

function makeJob(name: string, opts: Partial<Job> = {}): Job {
  const ts = new Date(Date.now() - Math.random() * 86400_000 * 7).toISOString();
  return {
    id: name,
    name,
    schedule: "0 */4 * * *",
    execution: { type: "ShellCommand", value: "echo " + name },
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
    next_run_at: new Date(Date.now() + Math.random() * 86400_000).toISOString(),
    ...opts,
  };
}

const FEW = [
  makeJob("backup-db"),
  makeJob("sync-users"),
  makeJob("health-check"),
  makeJob("nightly-report", { last_exit_code: 1 }),
  makeJob("legacy-cron", { enabled: false }),
];

const MANY: Job[] = Array.from({ length: 50 }, (_, i) =>
  makeJob(`task-${String(i).padStart(2, "0")}`),
);

export const Default: Story = {
  args: { jobs: FEW, loading: false },
  decorators: [
    (Story) => (
      <div style={{ padding: 32, maxWidth: 960 }}>
        <Story />
      </div>
    ),
  ],
};

export const Empty: Story = {
  args: { jobs: [], loading: false },
  decorators: [
    (Story) => (
      <div style={{ padding: 32, maxWidth: 960 }}>
        <Story />
      </div>
    ),
  ],
};

export const ManyJobs: Story = {
  args: { jobs: MANY, loading: false },
  decorators: [
    (Story) => (
      <div style={{ padding: 32, maxWidth: 960 }}>
        <Story />
      </div>
    ),
  ],
};
