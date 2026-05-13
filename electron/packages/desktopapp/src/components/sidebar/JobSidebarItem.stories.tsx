import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobSidebarItem } from "./JobSidebarItem";
import type { Job, JobRun } from "@/apis/types";

const meta: Meta<typeof JobSidebarItem> = {
  title: "Components/Sidebar/JobSidebarItem",
  component: JobSidebarItem,
};
export default meta;

type Story = StoryObj<typeof JobSidebarItem>;

const SAMPLE_JOB: Job = {
  id: "job-1",
  name: "backup-db",
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
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  version: 1,
  last_run_at: new Date().toISOString(),
  last_run_id: null,
  last_run_status: "Completed",
  next_run_at: null,
};

function run(status: JobRun["status"], id: string = Math.random().toString(36).slice(2)): JobRun {
  const now = new Date().toISOString();
  return {
    run_id: id,
    workflow_id: SAMPLE_JOB.id,
    workflow_version: 1,
    workflow_snapshot: { id: SAMPLE_JOB.id, name: SAMPLE_JOB.name, version: 1 },
    started_at: now,
    finished_at: status === "Running" ? null : now,
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
  args: {
    job: SAMPLE_JOB,
    runs: [
      run("Running"),
      run("Completed"),
      run("Completed"),
      run("Failed"),
      run("Completed"),
      run("Completed"),
      run("CompletedWithWarnings"),
    ],
  },
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};

export const NoRuns: Story = {
  args: { job: SAMPLE_JOB, runs: [] },
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};

export const MixedStatuses: Story = {
  args: {
    job: SAMPLE_JOB,
    runs: [
      run("Completed"),
      run("Failed"),
      run("Running"),
      run("Killed"),
      run("CompletedWithWarnings"),
      run("Completed"),
      run("Failed"),
    ],
  },
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};

export const FewerThanMax: Story = {
  args: {
    job: SAMPLE_JOB,
    runs: [run("Completed"), run("Failed")],
  },
  decorators: [
    (Story) => (
      <div style={{ width: 280, padding: 12 }}>
        <Story />
      </div>
    ),
  ],
};
