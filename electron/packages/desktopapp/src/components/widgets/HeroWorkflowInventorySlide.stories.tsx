import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { HeroWorkflowInventorySlide } from "./HeroWorkflowInventorySlide";
import type { Job } from "@/apis/types";

const meta: Meta<typeof HeroWorkflowInventorySlide> = {
  title: "Components/Widgets/HeroWorkflowInventorySlide",
  component: HeroWorkflowInventorySlide,
  parameters: { layout: "padded" },
  decorators: [
    (Story) => (
      <div style={{ width: 600, height: 420 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;
type Story = StoryObj<typeof HeroWorkflowInventorySlide>;

function makeJob(name: string, opts: Partial<Job> = {}): Job {
  const ts = new Date().toISOString();
  return {
    id: name,
    name,
    schedule: "0 */4 * * *",
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
    ...opts,
  };
}

const JOBS: Job[] = [
  makeJob("backup-db", { is_favorited: true }),
  makeJob("sync-users"),
  makeJob("health-check", { is_favorited: true }),
  makeJob("nightly-report"),
  makeJob("legacy-cron", { enabled: false }),
  makeJob("old-task", { enabled: false }),
  makeJob("scheduled-merge"),
  makeJob("audit-log-roll"),
];

export const Populated: Story = { args: { jobs: JOBS, loading: false } };
export const Empty: Story = { args: { jobs: [], loading: false } };
export const Loading: Story = { args: { jobs: [], loading: true } };
