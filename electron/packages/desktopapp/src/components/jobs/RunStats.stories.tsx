import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { RunStats } from "./RunStats";
import type { JobRun, WorkflowRunStep } from "@/apis/types";

/**
 * RunStats stories
 *
 * Default — a populated run with real cost + tokens.
 * EmptyPending — a freshly-started run with zero totals ("—" fallbacks).
 */

const meta: Meta<typeof RunStats> = {
  title: "Components/Jobs/RunStats",
  component: RunStats,
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

type Story = StoryObj<typeof RunStats>;

const JOB_ID = "11111111-2222-3333-4444-555555555555";

function makeStep(): WorkflowRunStep {
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
  };
}

function makeRun(overrides: Partial<JobRun>): JobRun {
  const startedAt = new Date(Date.now() - 5 * 60_000).toISOString();
  return {
    run_id: "9f3c2a17-04b8-4d6e-8a11-cafe0102dead",
    workflow_id: JOB_ID,
    workflow_version: 1,
    workflow_snapshot: { id: JOB_ID, name: "nightly-cleanup", version: 1 },
    started_at: startedAt,
    finished_at: new Date(Date.now() - 5 * 60_000 + 83_000).toISOString(),
    status: "Completed",
    trigger_input: null,
    steps: [makeStep()],
    total_cost_usd: 0.042,
    total_duration_ms: 83_000,
    total_input_tokens: 12_400,
    total_output_tokens: 3_800,
    ...overrides,
  };
}

export const Default: Story = {
  args: {
    run: makeRun({}),
  },
};

export const EmptyPending: Story = {
  args: {
    run: makeRun({
      status: "Running",
      finished_at: null,
      total_cost_usd: null,
      total_duration_ms: 0,
      total_input_tokens: 0,
      total_output_tokens: 0,
    }),
  },
};
