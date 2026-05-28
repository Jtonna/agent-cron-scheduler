import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { WorkflowHeader } from "./WorkflowHeader";
import type { Job } from "@/apis/types";

/**
 * WorkflowHeader stories
 *
 * Visual variants for the workflow detail page header: enabled (default),
 * disabled, favorited, and a long-name truncation check.
 */

function makeJob(overrides: Partial<Job> = {}): Job {
  const ts = new Date().toISOString();
  return {
    id: "weather-shell-claude",
    name: "weather-shell-claude",
    schedule: "0 * * * *",
    schedule_mode: "Cron",
    enabled: true,
    is_favorited: false,
    allow_concurrent: false,
    on_failure: "abort",
    steps: [],
    timezone: "America/Los_Angeles",
    working_dir: ".",
    env_vars: null,
    default_input: null,
    created_at: ts,
    updated_at: ts,
    version: 1,
    last_run_at: null,
    last_run_id: null,
    last_run_status: null,
    next_run_at: null,
    ...overrides,
  };
}

const meta: Meta<typeof WorkflowHeader> = {
  title: "Components/Workflows/WorkflowHeader",
  component: WorkflowHeader,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div style={{ width: 960 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof WorkflowHeader>;

export const Default: Story = {
  args: { job: makeJob() },
};

export const Favorited: Story = {
  args: { job: makeJob({ is_favorited: true }) },
};

export const Disabled: Story = {
  args: { job: makeJob({ enabled: false }) },
};

export const LongName: Story = {
  args: {
    job: makeJob({
      name: "a-rather-extraordinarily-long-workflow-name-that-must-truncate-here",
    }),
  },
};
