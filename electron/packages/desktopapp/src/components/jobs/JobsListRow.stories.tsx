import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobsListRow } from "./JobsListRow";
import type { Job, RecentRunEntry } from "@/apis/types";

const meta: Meta<typeof JobsListRow> = {
  title: "Components/Jobs/JobsListRow",
  component: JobsListRow,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof JobsListRow>;

function makeJob(overrides: Partial<Job>): Job {
  const ts = new Date().toISOString();
  return {
    id: "j1",
    name: "backup-db",
    schedule: "0 0 * * *",
    execution: { type: "ShellCommand", value: "echo backup" },
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
    last_run_at: new Date(Date.now() - 600_000).toISOString(),
    last_exit_code: 0,
    next_run_at: new Date(Date.now() + 3_600_000).toISOString(),
    ...overrides,
  };
}

function makeRun(status: RecentRunEntry["status"], offsetMs: number = 0): RecentRunEntry {
  const started = new Date(Date.now() - offsetMs).toISOString();
  return {
    run_id: Math.random().toString(36).slice(2),
    job_id: "j1",
    job_name: "backup-db",
    started_at: started,
    finished_at: status === "Running" ? null : started,
    status,
    exit_code: status === "Completed" ? 0 : 1,
    log_size_bytes: 0,
    error: null,
  };
}

export const Default: Story = {
  args: { job: makeJob({}) },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};

export const Disabled: Story = {
  args: {
    job: makeJob({ enabled: false, last_exit_code: null, next_run_at: null }),
  },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};

export const NeverRun: Story = {
  args: {
    job: makeJob({
      name: "fresh-job",
      last_run_at: null,
      last_exit_code: null,
    }),
  },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};

export const Failed: Story = {
  args: {
    job: makeJob({ name: "broken-task", last_exit_code: 1 }),
  },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};

export const Running: Story = {
  args: {
    job: makeJob({ name: "long-running-job" }),
    runs: [
      makeRun("Running", 0),
      makeRun("Completed", 60_000),
      makeRun("Completed", 120_000),
      makeRun("Failed", 180_000),
      makeRun("Completed", 240_000),
    ],
  },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};

export const Warning: Story = {
  args: {
    job: makeJob({ name: "flaky-job" }),
    runs: [
      makeRun("CompletedWithWarnings", 30_000),
      makeRun("Completed", 60_000),
      makeRun("CompletedWithWarnings", 120_000),
      makeRun("Completed", 180_000),
      makeRun("Completed", 240_000),
    ],
  },
  decorators: [
    (Story) => (
      <div style={{ width: 720 }}>
        <Story />
      </div>
    ),
  ],
};
