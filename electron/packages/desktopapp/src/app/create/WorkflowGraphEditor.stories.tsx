/**
 * WorkflowGraphEditor stories
 *
 * Visualises the shared editor used by /create and /workflows/[id]/edit:
 *   - Empty: just a single seeded shell step (what /create boots into).
 *   - FresnoWeather: a multi-step `create` example mirroring the
 *     `examples/fresno-weather-workflow.json` pattern.
 *   - EditMode: seeds the editor in `edit` mode with a fully-formed
 *     `Job` fixture so the "Save Changes" copy and the edit-mode header
 *     are exercised.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { WorkflowGraphEditor } from "./WorkflowGraphEditor";
import { makeDefaultStep, type NewWorkflow } from "./types";
import type { Job } from "@/apis/types";

const meta: Meta<typeof WorkflowGraphEditor> = {
  title: "Pages/CreateWorkflow",
  component: WorkflowGraphEditor,
  parameters: {
    layout: "fullscreen",
  },
};
export default meta;

type Story = StoryObj<typeof WorkflowGraphEditor>;

const EMPTY: NewWorkflow = {
  name: "",
  schedule: "0 9 * * *",
  timezone: "America/Los_Angeles",
  enabled: true,
  steps: [makeDefaultStep("shell")],
};

const FRESNO: NewWorkflow = {
  name: "fresno-weather-summary",
  schedule: "0 9 * * *",
  timezone: "America/Los_Angeles",
  enabled: true,
  steps: [
    {
      id: "fetch_weather",
      kind: "http",
      method: "GET",
      url: "https://api.open-meteo.com/v1/forecast?latitude=36.74&longitude=-119.78&current=temperature_2m",
      expect_status: [200],
    },
    {
      id: "check_temp",
      kind: "match",
      expr: "${steps.fetch_weather.parsed.current.temperature_2m > 35}",
      cases: {
        "true": [
          {
            id: "log_heat",
            kind: "shell",
            command: "echo 'It is HOT in Fresno today'",
          },
        ],
        "false": [
          {
            id: "log_mild",
            kind: "shell",
            command: "echo 'Pleasant in Fresno today'",
          },
        ],
      },
    },
    {
      id: "summarise",
      kind: "agent",
      agent_type: "claude_code_cli",
      prompt:
        "Summarize today's Fresno weather in one cheerful sentence. The raw forecast JSON is on stdin.",
      extra_args: [],
    },
  ],
};

/**
 * Server-shape fixture — the `Job` read model has more fields than
 * `NewWorkflow`, all of which the editor strips on seed. Cast through
 * `unknown` because the storybook-only fixture intentionally uses richer
 * step kinds than the read-side `WorkflowStep` union enumerates.
 */
const EXISTING_JOB: Job = {
  id: "wf_existing_123",
  name: "weather-greeter",
  schedule: "0 8 * * *",
  schedule_mode: "Cron",
  enabled: true,
  is_favorited: false,
  allow_concurrent: false,
  on_failure: "abort",
  timezone: "America/Los_Angeles",
  working_dir: "",
  env_vars: null,
  default_input: null,
  created_at: "2025-01-01T00:00:00Z",
  updated_at: "2025-01-02T00:00:00Z",
  version: 3,
  last_run_at: "2025-01-02T08:00:00Z",
  last_run_id: "run_abc",
  last_run_status: "Completed",
  next_run_at: "2025-01-03T08:00:00Z",
  steps: [
    {
      id: "fetch",
      kind: "http",
      method: "GET",
      url: "https://api.example.com/weather",
      headers: null,
      body: null,
      expect_status: [200],
      always_run: false,
      on_failure: "abort",
      timeout_secs: null,
      working_dir: null,
      env_vars: null,
      capture: {},
    },
    {
      id: "greet",
      kind: "shell",
      command: "echo 'good morning'",
      pass_stdin: false,
      always_run: false,
      on_failure: "abort",
      timeout_secs: null,
      working_dir: null,
      env_vars: null,
      capture: {},
    },
  ],
};

export const Empty: Story = {
  args: {
    mode: "create",
    initialWorkflow: EMPTY,
  },
};

export const FresnoWeather: Story = {
  args: {
    mode: "create",
    initialWorkflow: FRESNO,
  },
};

export const EditMode: Story = {
  args: {
    mode: "edit",
    workflowId: EXISTING_JOB.id,
    initialWorkflow: EXISTING_JOB,
  },
};
