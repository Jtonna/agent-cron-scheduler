/**
 * HttpStepBody stories — empty + filled.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { HttpStepBody } from "./HttpStepBody";
import { makeDefaultStep, type NewHttpStep } from "../types";

const meta: Meta<typeof HttpStepBody> = {
  title: "Pages/CreateWorkflow/Editors/HttpStepBody",
  component: HttpStepBody,
  parameters: { layout: "padded" },
};
export default meta;

type Story = StoryObj<typeof HttpStepBody>;

function Harness({ initial }: { initial: NewHttpStep }) {
  const [v, setV] = useState(initial);
  return (
    <div className="max-w-xl p-4 bg-surface-secondary border border-border rounded-menu">
      <HttpStepBody value={v} onChange={setV} />
    </div>
  );
}

export const Empty: Story = {
  render: () => <Harness initial={makeDefaultStep("http") as NewHttpStep} />,
};

export const Filled: Story = {
  render: () => (
    <Harness
      initial={{
        ...(makeDefaultStep("http") as NewHttpStep),
        method: "POST",
        url: "https://api.weather.gov/points/40.7128,-74.0060/forecast",
        headers: { Accept: "application/geo+json", "User-Agent": "acs/1.0" },
        body: '{ "format": "compact" }',
        expect_status: [200, 201],
        timeout_secs: 30,
      }}
    />
  ),
};
