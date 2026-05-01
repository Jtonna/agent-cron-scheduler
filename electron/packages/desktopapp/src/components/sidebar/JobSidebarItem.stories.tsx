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
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  last_run_at: new Date().toISOString(),
  last_exit_code: 0,
  next_run_at: null,
};

function run(status: JobRun["status"], id: string = Math.random().toString(36).slice(2)): JobRun {
  return {
    run_id: id,
    job_id: SAMPLE_JOB.id,
    started_at: new Date().toISOString(),
    finished_at: new Date().toISOString(),
    status,
    exit_code: status === "Completed" ? 0 : 1,
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
