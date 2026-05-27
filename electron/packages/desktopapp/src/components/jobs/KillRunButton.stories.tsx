import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { KillRunButton } from "./KillRunButton";

/**
 * KillRunButton stories
 *
 * `Default` shows the idle state at `sm` and `md` sizes; `Killing` is the
 * Storybook-only spinning variant (in production this state is driven by
 * the live mutation).
 */

const meta: Meta<typeof KillRunButton> = {
  title: "Components/Jobs/KillRunButton",
  component: KillRunButton,
  parameters: { layout: "centered" },
};
export default meta;

type Story = StoryObj<typeof KillRunButton>;

const RUN_ID = "9f3c2a17-04b8-4d6e-8a11-cafe0102dead";

export const Default: Story = {
  args: {
    runId: RUN_ID,
    size: "md",
  },
};

export const Small: Story = {
  args: {
    runId: RUN_ID,
    size: "sm",
  },
};
