/**
 * StepEditorModal stories
 *
 * One story per step kind (6), plus the special-case stories called out
 * in the redesign brief: empty state, kind-switcher open, advanced
 * expanded (mocked via initial state), and nested match modal.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { StepEditorModal } from "./StepEditorModal";
import { makeDefaultStep, type NewStep } from "./types";

const meta: Meta<typeof StepEditorModal> = {
  title: "Pages/CreateWorkflow/StepEditorModal",
  component: StepEditorModal,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof StepEditorModal>;

/** Bare wrapper that holds local state so each story is interactive. */
function Harness({
  initial,
  breadcrumb,
  withNested = false,
}: {
  initial: NewStep;
  breadcrumb?: { label: string }[];
  withNested?: boolean;
}) {
  const [step, setStep] = useState<NewStep>(initial);
  const [nested, setNested] = useState<NewStep | null>(null);
  return (
    <div className="min-h-screen bg-surface-secondary">
      <StepEditorModal
        step={step}
        breadcrumb={breadcrumb}
        onChange={setStep}
        onClose={() => {}}
        onOpenNested={
          withNested
            ? (key) => {
                if (step.kind !== "match") return;
                const child =
                  (key === null ? step.default : step.cases[key])?.[0] ??
                  makeDefaultStep("shell");
                setNested(child);
              }
            : undefined
        }
      />
      {nested && (
        <StepEditorModal
          step={nested}
          breadcrumb={[{ label: step.id }, { label: "hot" }]}
          onChange={setNested}
          onClose={() => setNested(null)}
        />
      )}
    </div>
  );
}

export const Shell: Story = {
  render: () => <Harness initial={makeDefaultStep("shell")} />,
};

export const Script: Story = {
  render: () => <Harness initial={makeDefaultStep("script")} />,
};

export const Http: Story = {
  render: () => <Harness initial={makeDefaultStep("http")} />,
};

export const Match: Story = {
  render: () => <Harness initial={makeDefaultStep("match")} />,
};

export const SetVar: Story = {
  render: () => <Harness initial={makeDefaultStep("set_var")} />,
};

export const Agent: Story = {
  render: () => <Harness initial={makeDefaultStep("agent")} />,
};

export const EmptyState: Story = {
  render: () => (
    <Harness
      initial={{
        ...makeDefaultStep("shell"),
        id: "",
        command: "",
      } as NewStep}
    />
  ),
};

export const AdvancedExpanded: Story = {
  render: () => (
    <Harness
      initial={{
        ...makeDefaultStep("shell"),
        always_run: true,
        timeout_secs: 120,
        working_dir: "/tmp",
        env_vars: { NODE_ENV: "production", DEBUG: "1" },
      } as NewStep}
    />
  ),
};

export const KindSwitcherOpen: Story = {
  render: () => (
    <div className="min-h-screen bg-surface-secondary">
      <p className="absolute top-3 left-3 z-50 text-[11px] text-fg-muted">
        Click the kind pill in the header to open the switcher popover.
      </p>
      <Harness initial={makeDefaultStep("set_var")} />
    </div>
  ),
};

/**
 * BiggerModalDemo
 *
 * Showcases the new Linear-style modal chrome (max-w-3xl, pt-[10vh],
 * max-h-[75vh], h-12 header) hosting a content-heavy match step with
 * several cases — the case rows benefit most from the extra width.
 */
export const BiggerModalDemo: Story = {
  render: () => (
    <Harness
      initial={
        {
          ...makeDefaultStep("match"),
          id: "route_by_status",
          expr: "${steps.fetch.status_code}",
          cases: {
            "200": [makeDefaultStep("shell")],
            "404": [makeDefaultStep("shell")],
            "500": [makeDefaultStep("shell")],
            "502": [makeDefaultStep("shell")],
            "503": [makeDefaultStep("shell")],
          },
          default: [makeDefaultStep("shell")],
        } as NewStep
      }
    />
  ),
};

export const NestedMatch: Story = {
  render: () => (
    <Harness
      withNested
      initial={
        {
          ...makeDefaultStep("match"),
          id: "route_mood",
          expr: "${steps.classify.exports.mood}",
          cases: {
            hot: [makeDefaultStep("shell")],
            cold: [makeDefaultStep("shell")],
          },
        } as NewStep
      }
    />
  ),
};
