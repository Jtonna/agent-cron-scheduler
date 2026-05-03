import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobRunsTable } from "./JobRunsTable";
import type { JobRun } from "@/apis/types";

/**
 * JobRunsTable stories
 *
 * Mock JobRun datasets covering the common states the table needs to handle
 * on the job detail page: a mix of statuses, all-green history, an in-flight
 * run, an empty state, and a loading state.
 */

const meta: Meta<typeof JobRunsTable> = {
  title: "Components/Jobs/JobRunsTable",
  component: JobRunsTable,
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

type Story = StoryObj<typeof JobRunsTable>;

const JOB_ID = "11111111-2222-3333-4444-555555555555";

function uuid(prefix: string): string {
  // 8-char prefix + filler; enough to render distinct short ids
  const filler = "aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee";
  const head = (prefix + "00000000").slice(0, 8);
  return `${head}-${filler.slice(9)}`;
}

function makeRun(
  prefix: string,
  status: JobRun["status"],
  startedOffsetMs: number,
  durationMs: number | null,
  cost: number | null,
  exitCode: number | null,
): JobRun {
  const startedAt = new Date(Date.now() - startedOffsetMs).toISOString();
  const finishedAt =
    status === "Running" || durationMs == null
      ? null
      : new Date(Date.now() - startedOffsetMs + durationMs).toISOString();
  return {
    run_id: uuid(prefix),
    job_id: JOB_ID,
    started_at: startedAt,
    finished_at: finishedAt,
    status,
    exit_code: exitCode,
    total_cost_usd: cost,
    duration_ms: durationMs,
  };
}

const MIXED_RUNS: JobRun[] = [
  makeRun("a1b2c3d4", "Running", 30_000, null, null, null),
  makeRun("b2c3d4e5", "Completed", 5 * 60_000, 134_000, 0.042, 0),
  makeRun("c3d4e5f6", "Failed", 35 * 60_000, 62_000, 0.018, 1),
  makeRun("d4e5f6a7", "Completed", 65 * 60_000, 8_000, 0.001, 0),
  makeRun("e5f6a7b8", "CompletedWithWarnings", 2 * 60 * 60_000, 271_000, 0.091, 0),
  makeRun("f6a7b8c9", "Completed", 4 * 60 * 60_000, 12_000, null, 0),
  makeRun("a7b8c9d0", "Killed", 6 * 60 * 60_000, 3_000, 0.002, 137),
  makeRun("b8c9d0e1", "Completed", 26 * 60 * 60_000, 22_000, 0.214, 0),
];

const ALL_SUCCEEDED: JobRun[] = Array.from({ length: 6 }).map((_, i) =>
  makeRun(
    "succ" + String(i).padStart(4, "0"),
    "Completed",
    (i + 1) * 30 * 60_000,
    Math.round(40_000 + Math.random() * 80_000),
    +(0.01 + Math.random() * 0.08).toFixed(4),
    0,
  ),
);

const WITH_RUNNING: JobRun[] = [
  makeRun("run01000", "Running", 12_000, null, null, null),
  makeRun("run02000", "Completed", 30 * 60_000, 90_000, 0.024, 0),
  makeRun("run03000", "Completed", 60 * 60_000, 110_000, 0.031, 0),
  makeRun("run04000", "Failed", 90 * 60_000, 4_000, 0.001, 2),
  makeRun("run05000", "Completed", 120 * 60_000, 76_000, 0.019, 0),
];

export const Default: Story = {
  args: { jobId: JOB_ID, runs: MIXED_RUNS },
};

export const OnlySucceeded: Story = {
  args: { jobId: JOB_ID, runs: ALL_SUCCEEDED },
  name: "Only succeeded",
};

export const WithRunning: Story = {
  args: { jobId: JOB_ID, runs: WITH_RUNNING },
  name: "With running",
};

export const Empty: Story = {
  args: { jobId: JOB_ID, runs: [] },
};

export const Loading: Story = {
  args: { jobId: JOB_ID, runs: [], loading: true },
};
