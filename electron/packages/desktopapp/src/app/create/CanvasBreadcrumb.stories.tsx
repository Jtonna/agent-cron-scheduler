/**
 * CanvasBreadcrumb stories.
 */

import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { CanvasBreadcrumb } from "./CanvasBreadcrumb";

const meta: Meta<typeof CanvasBreadcrumb> = {
  title: "Pages/CreateWorkflow/CanvasBreadcrumb",
  component: CanvasBreadcrumb,
  parameters: { layout: "fullscreen" },
};
export default meta;

type Story = StoryObj<typeof CanvasBreadcrumb>;

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
