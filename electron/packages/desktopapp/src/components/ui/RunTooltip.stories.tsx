import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { RunTooltip } from "./RunTooltip";
import type { JobRun } from "@/apis/types";

const meta: Meta<typeof RunTooltip> = {
  title: "Components/UI/RunTooltip",
  component: RunTooltip,
  parameters: {
    layout: "fullscreen",
  },
};
export default meta;

type Story = StoryObj<typeof RunTooltip>;

interface MockRun {
  run_id: string;
  status: JobRun["status"];
  started_at: string;
  total_cost_usd?: number | null;
}

const BASE_RUN: MockRun = {
  run_id: "01h9a8b7c6d5e4f3g2h1j0k9l8m7n6",
  status: "Completed",
  started_at: "2026-04-28T09:14:00.000Z",
  total_cost_usd: 0,
};

export const Default: Story = {
  args: {
    run: BASE_RUN,
    jobName: "backup-db",
    left: 100,
    top: 100,
  },
};

export const Failed: Story = {
  args: {
    run: { ...BASE_RUN, status: "Failed" },
    jobName: "deploy-staging",
    left: 100,
    top: 100,
  },
};

export const Running: Story = {
  args: {
    run: { ...BASE_RUN, status: "Running", started_at: new Date().toISOString() },
    jobName: "long-running-job",
    left: 100,
    top: 100,
  },
};

export const Warning: Story = {
  args: {
    run: { ...BASE_RUN, status: "CompletedWithWarnings" },
    jobName: "flaky-job",
    left: 100,
    top: 100,
  },
};

export const WithCost: Story = {
  args: {
    run: { ...BASE_RUN, total_cost_usd: 0.214 },
    jobName: "nightly-report",
    left: 100,
    top: 100,
  },
};

export const Killed: Story = {
  args: {
    run: { ...BASE_RUN, status: "Killed" },
    jobName: "runaway-task",
    left: 100,
    top: 100,
  },
};
