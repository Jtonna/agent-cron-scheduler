import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { RunDetailSidebar } from "./RunDetailSidebar";
import type { WorkflowRunStep } from "@/apis/types";

/**
 * RunDetailSidebar stories
 *
 * Mock step datasets cover: a partial run (5 of 7 steps ran, one active)
 * and a complete run where every step ran (3 of 3, no active selection).
 */

const meta: Meta<typeof RunDetailSidebar> = {
  title: "Components/Sidebar/RunDetailSidebar",
  component: RunDetailSidebar,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <div
        style={{ width: 280, height: 600 }}
        className="border-r border-border-subtle bg-surface text-fg"
      >
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof RunDetailSidebar>;

const JOB_ID = "11111111-2222-3333-4444-555555555555";

function makeStep(overrides: Partial<WorkflowRunStep>): WorkflowRunStep {
  return {
    step_index: 0,
    step_id: "step-1",
    kind: "shell",
    status: "Completed",
    started_at: new Date(Date.now() - 60_000).toISOString(),
    finished_at: new Date(Date.now() - 50_000).toISOString(),
    exit_code: 0,
    log_byte_offset_start: 0,
    log_byte_offset_end: 0,
    cost_usd: null,
    error: null,
    ...overrides,
  };
}

const FIVE_OF_SEVEN: WorkflowRunStep[] = [
  makeStep({ step_index: 0, step_id: "fetch-data", kind: "http" }),
  makeStep({
    step_index: 1,
    step_id: "summarize-with-claude",
    kind: "agent",
    cost_usd: 0.034,
    started_at: new Date(Date.now() - 200_000).toISOString(),
    finished_at: new Date(Date.now() - 80_000).toISOString(),
  }),
  makeStep({ step_index: 2, step_id: "render-report", kind: "shell" }),
  makeStep({ step_index: 3, step_id: "upload-artifact", kind: "shell" }),
  makeStep({
    step_index: 4,
    step_id: "post-to-slack",
    kind: "shell",
    status: "Failed",
    exit_code: 1,
  }),
];

const THREE_OF_THREE: WorkflowRunStep[] = [
  makeStep({ step_index: 0, step_id: "checkout", kind: "shell" }),
  makeStep({
    step_index: 1,
    step_id: "run-tests",
    kind: "shell",
    cost_usd: 0.002,
  }),
  makeStep({ step_index: 2, step_id: "publish", kind: "http" }),
];

export const Default: Story = {
  args: {
    jobId: JOB_ID,
    workflowName: "nightly-cleanup",
    cron: "0 2 * * *",
    runSteps: FIVE_OF_SEVEN,
    totalSteps: 7,
    activeStepIndex: 1,
    onSelectStep: () => {
      // no-op for stories
    },
  },
};

export const AllRan: Story = {
  args: {
    jobId: JOB_ID,
    workflowName: "release-build",
    cron: "@daily",
    runSteps: THREE_OF_THREE,
    totalSteps: 3,
    activeStepIndex: null,
    onSelectStep: () => {
      // no-op for stories
    },
  },
};
