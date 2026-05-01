import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobStateIndicator, type JobState, type JobStateSize } from "./JobStateIndicator";

const meta: Meta<typeof JobStateIndicator> = {
  title: "Components/UI/JobStateIndicator",
  component: JobStateIndicator,
};
export default meta;

type Story = StoryObj<typeof JobStateIndicator>;

const ALL_STATES: JobState[] = ["running", "success", "failed", "killed", "warning", "idle"];

const ALL_SIZES: JobStateSize[] = ["xs", "sm", "md", "lg"];

function Row({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="flex items-center gap-4">
      <span className="text-xs font-mono text-fg-muted w-20 capitalize">{title}</span>
      {children}
    </div>
  );
}

export const AllStatesDot: Story = {
  render: () => (
    <div className="flex flex-col gap-3 p-4">
      {ALL_STATES.map((state) => (
        <Row key={state} title={state}>
          <JobStateIndicator state={state} variant="dot" size="md" />
        </Row>
      ))}
    </div>
  ),
};

export const AllStatesBadge: Story = {
  render: () => (
    <div className="flex flex-col gap-3 p-4 items-start">
      {ALL_STATES.map((state) => (
        <JobStateIndicator key={state} state={state} variant="badge" />
      ))}
    </div>
  ),
};

export const AllStatesLabel: Story = {
  render: () => (
    <div className="flex flex-col gap-2 p-4">
      {ALL_STATES.map((state) => (
        <JobStateIndicator key={state} state={state} variant="label" />
      ))}
    </div>
  ),
};

export const Sizes: Story = {
  render: () => (
    <div className="flex flex-col gap-3 p-4">
      {ALL_SIZES.map((size) => (
        <Row key={size} title={size}>
          <JobStateIndicator state="success" variant="dot" size={size} />
          <JobStateIndicator state="failed" variant="dot" size={size} />
          <JobStateIndicator state="running" variant="dot" size={size} />
          <JobStateIndicator state="warning" variant="dot" size={size} />
          <JobStateIndicator state="killed" variant="dot" size={size} />
          <JobStateIndicator state="idle" variant="dot" size={size} />
        </Row>
      ))}
    </div>
  ),
};

export const RunningPulse: Story = {
  render: () => (
    <div className="flex items-center gap-4 p-4">
      <JobStateIndicator state="running" variant="dot" size="lg" />
      <span className="text-sm text-fg-muted">Auto-pulses on the running state</span>
    </div>
  ),
};

export const Idle: Story = {
  render: () => (
    <div className="flex items-center gap-4 p-4">
      <JobStateIndicator state="idle" variant="dot" size="md" />
      <JobStateIndicator state="idle" variant="label" />
      <JobStateIndicator state="idle" variant="badge" />
    </div>
  ),
};
