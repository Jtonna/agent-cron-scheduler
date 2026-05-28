import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { RunWithOverridesModal } from "./RunWithOverridesModal";
import type { Job, ShellWorkflowStep } from "@/apis/types";

const meta: Meta<typeof RunWithOverridesModal> = {
  title: "Components/Jobs/RunWithOverridesModal",
  component: RunWithOverridesModal,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof RunWithOverridesModal>;

function makeShellStep(id: string): ShellWorkflowStep {
  return {
    id,
    kind: "shell",
    command: `echo ${id}`,
    pass_stdin: false,
    always_run: false,
    on_failure: "abort",
    timeout_secs: null,
    working_dir: null,
    env_vars: null,
    capture: {},
  };
}

const BASE_JOB: Job = {
  id: "backup-db",
  name: "backup-db",
  schedule: "0 2 * * *",
  schedule_mode: "Cron",
  enabled: true,
  is_favorited: false,
  allow_concurrent: false,
  on_failure: "abort",
  steps: [makeShellStep("dump"), makeShellStep("compress"), makeShellStep("upload")],
  timezone: "UTC",
  working_dir: ".",
  env_vars: { DB_HOST: "localhost", DB_USER: "postgres" },
  default_input: { target: "primary", verbose: true },
  created_at: new Date().toISOString(),
  updated_at: new Date().toISOString(),
  version: 1,
  last_run_at: null,
  last_run_id: null,
  last_run_status: null,
  next_run_at: null,
};

const NO_STEPS_JOB: Job = { ...BASE_JOB, steps: [], default_input: null, env_vars: null };

export const Default: Story = {
  args: {
    workflow: BASE_JOB,
    isOpen: true,
    onOpenChange: () => {},
  },
};

export const NoStepsNoDefaults: Story = {
  args: {
    workflow: NO_STEPS_JOB,
    isOpen: true,
    onOpenChange: () => {},
  },
};

export const Closed: Story = {
  args: {
    workflow: BASE_JOB,
    isOpen: false,
    onOpenChange: () => {},
  },
};
