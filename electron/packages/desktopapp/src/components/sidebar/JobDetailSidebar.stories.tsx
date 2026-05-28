import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobDetailSidebar } from "./JobDetailSidebar";
import type { Job } from "@/apis/types";

const meta: Meta<typeof JobDetailSidebar> = {
  title: "Components/Sidebar/JobDetailSidebar",
  component: JobDetailSidebar,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <div
        style={{
          width: 280,
          borderRight: "1px solid var(--color-border-subtle)",
          height: "100vh",
        }}
      >
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof JobDetailSidebar>;

function makeJob(name: string, daysAgo: number, overrides: Partial<Job> = {}): Job {
  const ts = new Date(Date.now() - daysAgo * 86400_000).toISOString();
  return {
    id: name,
    name,
    schedule: "0 2 * * *",
    schedule_mode: "Cron",
    enabled: true,
    is_favorited: false,
    allow_concurrent: false,
    on_failure: "abort",
    steps: [],
    timezone: "UTC",
    working_dir: ".",
    env_vars: null,
    default_input: null,
    created_at: ts,
    updated_at: ts,
    version: 1,
    last_run_at: ts,
    last_run_id: null,
    last_run_status: "Completed",
    next_run_at: null,
    ...overrides,
  };
}

const CURRENT = makeJob("backup-db", 0);

/**
 * Default: enabled job, not favorited. The action area is a split button —
 * "Run Workflow" on the left, a chevron trigger on the right that opens
 * a menu offering "Run with customizations…". Both halves are joined
 * into a single `rounded-input` shape (the shadcn button-group / IDE-Run
 * pattern). All other controls share the same square shape — intent is
 * what changes, not shape.
 */
export const Default: Story = {
  args: { job: CURRENT },
};

/**
 * SplitMenuOpen — programmatically opens the chevron menu so we can
 * eyeball the popover styling and keyboard target sizing without having
 * to click. The menu lives in a portal, so it floats over the rail.
 */
export const SplitMenuOpen: Story = {
  args: { job: CURRENT },
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>(
      'button[aria-label="More run options"]',
    );
    trigger?.click();
  },
};

/**
 * Triggering — primary half shows a spinner and the "Running…" label;
 * the chevron half is disabled so a stray click can't open the menu
 * while a run is being dispatched.
 */
export const Triggering: Story = {
  args: { job: CURRENT },
  parameters: {
    docs: {
      description: {
        story:
          "Visual reference only — the actual triggering state is driven by useTriggerWorkflow and isn't exercised in Storybook.",
      },
    },
  },
};

/**
 * Favorited variant — the star icon in the identity block fills in with
 * the brand color. No other layout changes; this exists so we can
 * eyeball the toggle visually in Storybook.
 */
export const Favorited: Story = {
  args: { job: makeJob("backup-db", 0, { is_favorited: true }) },
};

/**
 * Disabled-cron variant — the Enable/Disable utility row label flips
 * from "Disable cron" to "Enable cron". Same shape, same intent.
 */
export const Disabled: Story = {
  args: { job: makeJob("backup-db", 0, { enabled: false }) },
};

/**
 * Long name — ensures the identity block truncates the workflow name
 * and the schedule line without breaking the layout of the buttons
 * below.
 */
export const LongName: Story = {
  args: {
    job: makeJob(
      "a-rather-extraordinarily-long-workflow-name-that-must-truncate",
      0,
    ),
  },
};

/**
 * Favorited + disabled — the combined state, useful for verifying that
 * the rail still reads cleanly when both metadata bits are flipped at
 * once.
 */
export const FavoritedAndDisabled: Story = {
  args: {
    job: makeJob("backup-db", 0, { is_favorited: true, enabled: false }),
  },
};
