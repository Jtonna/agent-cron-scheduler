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
 * Default: enabled job, not favorited. Shows all action buttons in their
 * default state: primary "Run Workflow" (solid, square), secondary
 * "Run with Customizations" (bordered, square), Enable/Disable cron
 * ghost row, Edit ghost row, and the destructive Delete row in the
 * pinned footer. All controls share the same `rounded-input` shape —
 * intent is what changes, not shape.
 */
export const Default: Story = {
  args: { job: CURRENT },
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
