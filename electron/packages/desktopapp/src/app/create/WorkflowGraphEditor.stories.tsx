/**
 * WorkflowGraphEditor stories
 *
 * Visualises the /create page editor with two fixtures:
 *   - Empty: just a single seeded shell step (what the page boots into).
 *   - FresnoWeather: a multi-step example mirroring the
 *     `examples/fresno-weather-workflow.json` pattern — a shell fetch,
 *     a match on its result, and an agent summarisation step.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { WorkflowGraphEditor } from "./WorkflowGraphEditor";
import { makeDefaultStep, type NewWorkflow } from "./types";

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

export const Empty: Story = {
  args: {
    initialWorkflow: EMPTY,
  },
};

export const FresnoWeather: Story = {
  args: {
    initialWorkflow: FRESNO,
  },
};
