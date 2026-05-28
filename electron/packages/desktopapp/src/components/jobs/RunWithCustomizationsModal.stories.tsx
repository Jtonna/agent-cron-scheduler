import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { RunWithCustomizationsModal } from "./RunWithCustomizationsModal";
import type {
  AgentWorkflowStep,
  HttpWorkflowStep,
  Job,
  ShellWorkflowStep,
} from "@/apis/types";

const meta: Meta<typeof RunWithCustomizationsModal> = {
  title: "Components/Workflows/RunWithCustomizationsModal",
  component: RunWithCustomizationsModal,
  parameters: { layout: "fullscreen" },
  argTypes: {
    isOpen: { control: "boolean" },
  },
};
export default meta;

type Story = StoryObj<typeof RunWithCustomizationsModal>;

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

function makeHttpStep(id: string): HttpWorkflowStep {
  return {
    id,
    kind: "http",
    method: "GET",
    url: `https://example.com/${id}`,
    headers: null,
    body: null,
    expect_status: null,
    always_run: false,
    on_failure: "abort",
    timeout_secs: null,
    working_dir: null,
    env_vars: null,
    capture: {},
  };
}

function makeAgentStep(id: string): AgentWorkflowStep {
  return {
    id,
    kind: "agent",
    agent_type: "claude",
    prompt: `Do ${id}`,
    model: null,
    extra_args: null,
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

const MIXED_KINDS_JOB: Job = {
  ...BASE_JOB,
  name: "mixed-pipeline",
  steps: [
    makeHttpStep("fetch-data"),
    makeShellStep("transform"),
    makeAgentStep("summarize"),
    makeShellStep("upload"),
  ],
};

const NO_STEPS_JOB: Job = { ...BASE_JOB, steps: [], default_input: null, env_vars: null };

const ONLY_NON_TARGETABLE_JOB: Job = {
  ...BASE_JOB,
  name: "agent-only",
  steps: [makeAgentStep("plan"), makeAgentStep("execute")],
};

export const Default: Story = {
  args: {
    workflow: BASE_JOB,
    isOpen: true,
    onOpenChange: () => {},
  },
};

export const MixedStepKinds: Story = {
  args: {
    workflow: MIXED_KINDS_JOB,
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

export const NoTargetableSteps: Story = {
  args: {
    workflow: ONLY_NON_TARGETABLE_JOB,
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
