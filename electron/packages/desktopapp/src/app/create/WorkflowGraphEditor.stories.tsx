/**
 * WorkflowGraphEditor stories
 *
 * Showcases the canvas-only surface of the editor (no sidebar chrome).
 * Each story wraps the editor in a stateful harness that supplies the
 * controlled identity props (name / schedule / timezone / enabled) the
 * page would normally hold on its behalf.
 *
 *   - EmptyCreate — fresh `/create` entry, one seeded shell step.
 *   - FilledCreate — multi-step `create` example mirroring the
 *     `examples/fresno-weather-workflow.json` pattern.
 *   - EditMode — seeds the editor in `edit` mode with a fully-formed
 *     `Job` fixture.
 *   - EditModeWithModalOpen — same as EditMode; viewers click a node to
 *     open the palette modal.
 *   - MatchStepNestedModal — workflow seeded with a match step;
 *     clicking the match node and drilling into a case opens the
 *     nested modal with breadcrumb.
 */

import { useState } from "react";
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

const NESTED_MATCH: NewWorkflow = {
  name: "route-mood-demo",
  schedule: "0 * * * *",
  timezone: "UTC",
  enabled: true,
  steps: [
    {
      id: "route_mood",
      kind: "match",
      expr: "${steps.classify.exports.mood}",
      cases: {
        hot: [
          {
            id: "page_oncall",
            kind: "shell",
            command: "echo paging oncall",
          },
        ],
        cold: [
          {
            id: "log_cold",
            kind: "shell",
            command: "echo cold path",
          },
        ],
      },
      default: [
        { id: "log_default", kind: "shell", command: "echo neutral" },
      ],
    },
  ],
};

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

/**
 * Stateful harness — supplies the controlled identity props the page
 * would normally manage, so the editor can render in isolation.
 */
function CreateHarness({ seed }: { seed: NewWorkflow }) {
  const [name, setName] = useState(seed.name);
  const [schedule, setSchedule] = useState(seed.schedule);
  const [timezone, setTimezone] = useState(seed.timezone ?? "");
  const [enabled, setEnabled] = useState(seed.enabled ?? true);
  // setters are intentionally unused inside the story — they exist
  // so the editor's controlled inputs are wired even if the sidebar
  // isn't part of the story.
  void setName;
  void setSchedule;
  void setTimezone;
  void setEnabled;
  return (
    <div className="h-screen flex flex-col bg-surface">
      <WorkflowGraphEditor
        mode="create"
        initialWorkflow={seed}
        name={name}
        schedule={schedule}
        timezone={timezone}
        enabled={enabled}
      />
    </div>
  );
}

function EditHarness({ workflowId, job }: { workflowId: string; job: Job }) {
  const [name, setName] = useState(job.name);
  const [schedule, setSchedule] = useState(job.schedule);
  const [timezone, setTimezone] = useState(job.timezone ?? "");
  const [enabled, setEnabled] = useState(job.enabled);
  void setName;
  void setSchedule;
  void setTimezone;
  void setEnabled;
  return (
    <div className="h-screen flex flex-col bg-surface">
      <WorkflowGraphEditor
        mode="edit"
        workflowId={workflowId}
        initialWorkflow={job}
        name={name}
        schedule={schedule}
        timezone={timezone}
        enabled={enabled}
      />
    </div>
  );
}

export const EmptyCreate: Story = {
  render: () => <CreateHarness seed={EMPTY} />,
};

export const FilledCreate: Story = {
  render: () => <CreateHarness seed={FRESNO} />,
};

export const EditMode: Story = {
  render: () => (
    <EditHarness workflowId={EXISTING_JOB.id} job={EXISTING_JOB} />
  ),
};

export const EditModeWithModalOpen: Story = {
  render: () => (
    <EditHarness workflowId={EXISTING_JOB.id} job={EXISTING_JOB} />
  ),
  parameters: {
    docs: {
      description: {
        story:
          "Click any step node in the canvas to open the palette-style step editor modal.",
      },
    },
  },
};

export const MatchStepNestedModal: Story = {
  render: () => <CreateHarness seed={NESTED_MATCH} />,
  parameters: {
    docs: {
      description: {
        story:
          "Click the `route_mood` match node, then click a case row's drill-in arrow to open the nested editor with breadcrumb.",
      },
    },
  },
};
