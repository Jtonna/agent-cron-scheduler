import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobDetailSidebar } from "./JobDetailSidebar";
import { CommandPaletteContext } from "@/components/command-palette/CommandPaletteContext";
import type { Job, JobRun } from "@/apis/types";

const fakePaletteCtx = {
  isOpen: false,
  open: () => {},
  close: () => {},
  toggle: () => {},
  registerCommands: () => "reg-0",
  unregisterCommands: () => {},
};

const meta: Meta<typeof JobDetailSidebar> = {
  title: "Components/Sidebar/JobDetailSidebar",
  component: JobDetailSidebar,
  parameters: { layout: "fullscreen" },
  decorators: [
    (Story) => (
      <CommandPaletteContext.Provider value={fakePaletteCtx}>
        <div
          style={{
            width: 300,
            borderRight: "1px solid var(--color-border-subtle)",
            height: "100vh",
          }}
        >
          <Story />
        </div>
      </CommandPaletteContext.Provider>
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
    schedule: "0 * * * *",
    schedule_mode: "Cron",
    enabled: true,
    is_favorited: false,
    allow_concurrent: false,
    on_failure: "abort",
    steps: [],
    timezone: "America/Los_Angeles",
    working_dir: ".",
    env_vars: null,
    default_input: null,
    created_at: ts,
    updated_at: ts,
    version: 1,
    last_run_at: new Date(Date.now() - 2 * 60_000).toISOString(),
    last_run_id: null,
    last_run_status: "Completed",
    next_run_at: null,
    ...overrides,
  };
}

function makeRun(
  index: number,
  status: JobRun["status"],
  minutesAgo: number,
): JobRun {
  const ts = new Date(Date.now() - minutesAgo * 60_000).toISOString();
  return {
    run_id: `run-${String(index).padStart(8, "0")}`,
    workflow_id: "weather-shell-claude",
    workflow_version: 1,
    workflow_snapshot: {
      id: "weather-shell-claude",
      name: "weather-shell-claude",
      version: 1,
    },
    started_at: ts,
    finished_at: ts,
    status,
    trigger_input: null,
    steps: [],
    total_cost_usd: null,
    total_duration_ms: 4200,
    total_input_tokens: 0,
    total_output_tokens: 0,
  };
}

const CURRENT = makeJob("weather-shell-claude", 0);
const SAMPLE_RUNS: JobRun[] = [
  makeRun(42, "Completed", 2),
  makeRun(41, "Failed", 60),
  makeRun(40, "Completed", 120),
  makeRun(39, "Killed", 240),
  makeRun(38, "Completed", 360),
  makeRun(37, "Completed", 480),
];

/**
 * Default — enabled workflow with a fresh run history. STATUS shows the
 * inline toggle in its "on" position; the search trigger sits at the top.
 */
export const Default: Story = {
  args: { job: CURRENT, runsOverride: SAMPLE_RUNS },
};

/**
 * Disabled — cron is off. The toggle reads as off and the palette label
 * for `toggle-cron` flips to "Enable Cron".
 */
export const Disabled: Story = {
  args: {
    job: makeJob("weather-shell-claude", 0, { enabled: false }),
    runsOverride: SAMPLE_RUNS,
  },
};

/**
 * Favorited — the star fills in with the brand color. Palette label
 * for `favorite-workflow` becomes "Unfavorite".
 */
export const Favorited: Story = {
  args: {
    job: makeJob("weather-shell-claude", 0, { is_favorited: true }),
    runsOverride: SAMPLE_RUNS,
  },
};

/**
 * LongName — verifies the identity block truncates a very long workflow
 * name and that the favorite + overflow controls stay aligned.
 */
export const LongName: Story = {
  args: {
    job: makeJob(
      "a-rather-extraordinarily-long-workflow-name-that-must-truncate-here",
      0,
    ),
    runsOverride: SAMPLE_RUNS,
  },
};

/**
 * EnableCronToggling — visual reference for the disabled toggle state.
 * The real pending state is driven by `useToggleWorkflowEnabled` and
 * isn't exercised inside Storybook.
 */
export const EnableCronToggling: Story = {
  args: { job: CURRENT, runsOverride: SAMPLE_RUNS },
  parameters: {
    docs: {
      description: {
        story:
          "Toggle pending visuals come from `useToggleWorkflowEnabled.toggling`; this story exists to keep the visual review surface intact.",
      },
    },
  },
};

/**
 * DeleteDialogOpen — uses the `play` hook to open the Delete dialog so we
 * can review the destructive confirmation styling without clicking.
 */
export const DeleteDialogOpen: Story = {
  args: { job: CURRENT, runsOverride: SAMPLE_RUNS },
  play: async ({ canvasElement }) => {
    const trigger = canvasElement.querySelector<HTMLButtonElement>(
      'button[aria-label="Delete workflow"]',
    );
    trigger?.click();
  },
};

/**
 * NoRuns — the empty state for the Recent Runs section.
 */
export const NoRuns: Story = {
  args: { job: CURRENT, runsOverride: [] },
};
