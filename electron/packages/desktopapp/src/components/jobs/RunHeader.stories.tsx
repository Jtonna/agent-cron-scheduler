import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { RunHeader } from "./RunHeader";

/**
 * RunHeader stories
 *
 * Three minimal variants matching the three header statuses the run-detail
 * page exercises: completed (Default), running, and failed.
 */

const meta: Meta<typeof RunHeader> = {
  title: "Components/Workflows/RunHeader",
  component: RunHeader,
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

type Story = StoryObj<typeof RunHeader>;

const RUN_ID = "9f3c2a17-04b8-4d6e-8a11-cafe0102dead";
const WORKFLOW_NAME = "nightly-cleanup";

export const Default: Story = {
  args: {
    workflowName: WORKFLOW_NAME,
    runId: RUN_ID,
    status: "Completed",
  },
};

export const Running: Story = {
  args: {
    workflowName: WORKFLOW_NAME,
    runId: RUN_ID,
    status: "Running",
  },
};

export const Failed: Story = {
  args: {
    workflowName: WORKFLOW_NAME,
    runId: RUN_ID,
    status: "Failed",
  },
};
