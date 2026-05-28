import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobsSidebar } from "./JobsSidebar";
import { JobDetailSidebar } from "./JobDetailSidebar";
import { RunDetailSidebar } from "./RunDetailSidebar";
import { CommandPaletteContext } from "@/components/command-palette/CommandPaletteContext";
import type { Job, JobRun, WorkflowRunStep } from "@/apis/types";

/**
 * Composition gallery — renders all three sidebars side by side so the
 * unified anatomy can be reviewed at a glance. Use this when tweaking
 * any of the shared sidebar primitives (SidebarSectionHeader,
 * SidebarListItem, SidebarBackLink, SidebarIdentityBlock) to confirm
 * the change reads the same way across every rail.
 */

const fakePaletteCtx = {
  isOpen: false,
  open: () => {},
  close: () => {},
  toggle: () => {},
  registerCommands: () => "reg-0",
  unregisterCommands: () => {},
};

const meta: Meta = {
  title: "Components/Sidebar/_Gallery",
  parameters: { layout: "fullscreen" },
};
export default meta;

function makeJob(name: string, overrides: Partial<Job> = {}): Job {
  const ts = new Date(Date.now() - 86_400_000).toISOString();
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

function makeRun(i: number, status: JobRun["status"], minsAgo: number): JobRun {
  const ts = new Date(Date.now() - minsAgo * 60_000).toISOString();
  return {
    run_id: `run-${String(i).padStart(8, "0")}`,
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

function makeStep(overrides: Partial<WorkflowRunStep>): WorkflowRunStep {
  return {
    step_index: 0,
    step_id: "step",
    kind: "shell",
    status: "Completed",
    started_at: new Date(Date.now() - 60_000).toISOString(),
    finished_at: new Date(Date.now() - 30_000).toISOString(),
    exit_code: 0,
    log_byte_offset_start: 0,
    log_byte_offset_end: 0,
    cost_usd: null,
    error: null,
    ...overrides,
  };
}

const JOBS: Job[] = [
  makeJob("backup-db", { is_favorited: true }),
  makeJob("sync-users"),
  makeJob("health-check"),
  makeJob("cleanup-logs"),
  makeJob("nightly-report"),
];

const RUNS: JobRun[] = [
  makeRun(42, "Completed", 2),
  makeRun(41, "Failed", 60),
  makeRun(40, "Completed", 120),
  makeRun(39, "Killed", 240),
  makeRun(38, "Completed", 360),
  makeRun(37, "Completed", 480),
];

const STEPS: WorkflowRunStep[] = [
  makeStep({ step_index: 0, step_id: "fetch-data", kind: "http" }),
  makeStep({
    step_index: 1,
    step_id: "summarize",
    kind: "agent",
    cost_usd: 0.034,
  }),
  makeStep({ step_index: 2, step_id: "publish", kind: "shell" }),
];

const FRAME: React.CSSProperties = {
  width: 300,
  height: "100vh",
  borderRight: "1px solid var(--color-border-subtle)",
};

export const AllThree: StoryObj = {
  render: () => (
    <CommandPaletteContext.Provider value={fakePaletteCtx}>
      <div className="flex bg-surface text-fg">
        <div style={FRAME}>
          <JobsSidebar jobs={JOBS} />
        </div>
        <div style={FRAME}>
          <JobDetailSidebar
            job={makeJob("weather-shell-claude", { is_favorited: true })}
            runsOverride={RUNS}
          />
        </div>
        <div style={FRAME}>
          <RunDetailSidebar
            jobId="weather-shell-claude"
            workflowName="weather-shell-claude"
            cron="0 * * * *"
            runSteps={STEPS}
            totalSteps={3}
            activeStepIndex={1}
            onSelectStep={() => {}}
          />
        </div>
      </div>
    </CommandPaletteContext.Provider>
  ),
};
