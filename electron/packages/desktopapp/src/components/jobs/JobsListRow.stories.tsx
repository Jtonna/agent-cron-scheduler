import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobsListRow } from "./JobsListRow";
import type { Job, RecentRunEntry } from "@/apis/types";

const meta: Meta<typeof JobsListRow> = {
  title: "Components/Workflows/JobsListRow",
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
    last_run_at: new Date(Date.now() - 600_000).toISOString(),
    last_run_id: null,
    last_run_status: "Completed",
    next_run_at: new Date(Date.now() + 3_600_000).toISOString(),
    ...overrides,
  };
}

function makeRun(status: RecentRunEntry["status"], offsetMs: number = 0): RecentRunEntry {
  const started = new Date(Date.now() - offsetMs).toISOString();
  return {
    run_id: Math.random().toString(36).slice(2),
    workflow_id: "j1",
    workflow_version: 1,
    workflow_snapshot: { id: "j1", name: "backup-db", version: 1 },
    started_at: started,
    finished_at: status === "Running" ? null : started,
    status,
    trigger_input: null,
    steps: [],
    total_cost_usd: null,
    total_duration_ms: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
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
    job: makeJob({ enabled: false, last_run_status: null, next_run_at: null }),
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
      last_run_status: null,
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
    job: makeJob({ name: "broken-task", last_run_status: "Failed" }),
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

export const Favorited: Story = {
  args: { job: makeJob({ name: "important-job", is_favorited: true }) },
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
