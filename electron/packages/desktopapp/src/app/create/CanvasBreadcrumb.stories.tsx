/**
 * CanvasBreadcrumb stories.
 *
 * Covers both crumb shapes — link/static crumbs and the new editable
 * crumb (used for the workflow name in the editor pages). The editable
 * crumb stories are interactive: an internal harness holds state so
 * typing into the input persists across keystrokes.
 */

import { useState } from "react";
import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { CanvasBreadcrumb } from "./CanvasBreadcrumb";

const meta: Meta<typeof CanvasBreadcrumb> = {
  title: "Pages/CreateWorkflow/CanvasBreadcrumb",
  component: CanvasBreadcrumb,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof CanvasBreadcrumb>;

function EditableHarness({ initial }: { initial: string }) {
  const [name, setName] = useState(initial);
  return (
    <CanvasBreadcrumb
      crumbs={[
        { label: "Workflows", href: "/workflows" },
        {
          kind: "editable",
          value: name,
          onChange: setName,
          placeholder: "New workflow",
          ariaLabel: "Edit workflow name",
        },
      ]}
    />
  );
}

export const NewWorkflow: Story = {
  args: { crumbs: [{ label: "Workflows", href: "/workflows" }, { label: "New workflow" }] },
};

export const EditWorkflow: Story = {
  args: {
    crumbs: [
      { label: "Workflows", href: "/workflows" },
      { label: "weather-greeter-demo", href: "/workflows/wf_123" },
      { label: "Edit" },
    ],
  },
};

/** Editable crumb — empty state, shows placeholder. Click to edit. */
export const EditableEmpty: Story = {
  render: () => <EditableHarness initial="" />,
};

/** Editable crumb — pre-filled with a name. Click to edit. */
export const EditableFilled: Story = {
  render: () => <EditableHarness initial="weather-greeter-demo" />,
};

/**
 * Editable crumb — pre-mounted into edit mode by focusing the input on
 * first paint. Storybook viewers see the input chrome directly without
 * needing to click first.
 */
export const EditableEditing: Story = {
  render: () => {
    function Auto() {
      const [name, setName] = useState("weather-greeter-demo");
      return (
        <div
          ref={(el) => {
            // After the breadcrumb mounts, click its editable trigger so
            // the input mode renders. Done as a ref-callback so we don't
            // depend on storybook's play() infra.
            if (!el) return;
            requestAnimationFrame(() => {
              const btn = el.querySelector<HTMLButtonElement>(
                'button[aria-label="Edit workflow name"]',
              );
              btn?.click();
            });
          }}
        >
          <CanvasBreadcrumb
            crumbs={[
              { label: "Workflows", href: "/workflows" },
              {
                kind: "editable",
                value: name,
                onChange: setName,
                placeholder: "New workflow",
                ariaLabel: "Edit workflow name",
              },
            ]}
          />
        </div>
      );
    }
    return <Auto />;
  },
};
