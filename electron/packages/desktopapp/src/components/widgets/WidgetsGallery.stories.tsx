import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { CostWidget } from "./CostWidget";
import { TokensWidget } from "./TokensWidget";
import { HealthWidget, type RunStatus } from "./HealthWidget";
import { TopSpendersWidget } from "./TopSpendersWidget";
import { CostTrendWidget } from "./CostTrendWidget";
import { JobCostWidget } from "./JobCostWidget";
import { JobCostTrendWidget } from "./JobCostTrendWidget";
import { NoirCallout } from "./NoirCallout";
import type {
  WorkflowCostSummary,
  WorkflowCostEntry,
  DailyCostBucket,
} from "@/apis/types";

/**
 * Widgets gallery — every redesigned dashboard / workflow tile in one
 * frame so the new visual language (pastel meshes, noir callouts, ink
 * chart palette) can be reviewed at a glance.
 */

const meta: Meta = {
  title: "Components/Widgets/_Gallery",
};
export default meta;

type Story = StoryObj;

// ──────────────────────────── fixtures ────────────────────────────

function dayKey(offset: number): string {
  const d = new Date();
  d.setDate(d.getDate() - offset);
  return d.toISOString().slice(0, 10);
}

const DAILY_BUCKETS: DailyCostBucket[] = Array.from({ length: 24 }, (_, i) => {
  const t = i / 23;
  const value = 1.2 + Math.sin(t * Math.PI * 2) * 0.9 + t * 2.4;
  return {
    date: dayKey(23 - i),
    runs_completed: 3,
    runs_failed: 0,
    runs_killed: 0,
    cost_from_completed: value,
    cost_from_failed: 0,
    cost_from_killed: 0,
    total_usd: value,
    tokens_in_from_completed: 12000,
    tokens_in_from_failed: 0,
    tokens_in_from_killed: 0,
    tokens_out_from_completed: 4000,
    tokens_out_from_failed: 0,
    tokens_out_from_killed: 0,
    total_input_tokens: 12000,
    total_output_tokens: 4000,
  };
});

const COST_SUMMARY: WorkflowCostSummary = {
  computed_at: new Date().toISOString(),
  last_30_days_runs: 184,
  last_30_days_total_usd: 184.32,
  last_year_runs: 2104,
  last_year_total_usd: 2_318.4,
  last_30_days_input_tokens: 2_400_000,
  last_30_days_output_tokens: 760_000,
  last_year_input_tokens: 31_200_000,
  last_year_output_tokens: 9_900_000,
  daily_buckets: DAILY_BUCKETS,
};

const WORKFLOWS: WorkflowCostEntry[] = [
  ["nightly-research", 92.41, 84],
  ["weekly-digest", 41.04, 28],
  ["pr-reviewer", 23.18, 96],
  ["spam-triage", 18.62, 240],
  ["calendar-summary", 8.74, 64],
].map(([name, usd, runs]) => ({
  workflow_id: String(name),
  workflow_name: String(name),
  cost_summary: {
    computed_at: new Date().toISOString(),
    last_30_days_runs: Number(runs),
    last_30_days_total_usd: Number(usd),
    last_year_runs: Number(runs) * 12,
    last_year_total_usd: Number(usd) * 12,
    last_30_days_input_tokens: 0,
    last_30_days_output_tokens: 0,
    last_year_input_tokens: 0,
    last_year_output_tokens: 0,
    daily_buckets: [],
  },
}));

interface FixtureRun {
  started_at: string;
  status: RunStatus;
}
function makeRun(status: RunStatus, daysAgo: number = 0): FixtureRun {
  return {
    started_at: new Date(Date.now() - daysAgo * 86400_000).toISOString(),
    status,
  };
}
const HEALTH_RUNS: FixtureRun[] = [
  ...Array.from({ length: 142 }, (_, i) => makeRun("Completed", i % 13)),
  ...Array.from({ length: 6 }, (_, i) => makeRun("Failed", i % 13)),
  ...Array.from({ length: 2 }, (_, i) => makeRun("Running", i % 13)),
  ...Array.from({ length: 3 }, (_, i) => makeRun("CompletedWithWarnings", i % 13)),
];

// ──────────────────────────── gallery ────────────────────────────

export const Dashboard: Story = {
  render: () => (
    <div className="bg-surface p-8 min-h-screen">
      <div className="max-w-[1280px] mx-auto flex flex-col gap-6">
        <h2 className="text-2xl font-extrabold tracking-tight text-fg">Dashboard widgets</h2>
        <div className="grid grid-cols-4 gap-4">
          <CostWidget summary={COST_SUMMARY} loading={false} />
          <TokensWidget summary={COST_SUMMARY} loading={false} />
          <HealthWidget runs={HEALTH_RUNS} />
          <TopSpendersWidget workflows={WORKFLOWS} />
        </div>
        <CostTrendWidget data={DAILY_BUCKETS} />
      </div>
    </div>
  ),
};

export const WorkflowDetail: Story = {
  render: () => (
    <div className="bg-surface p-8 min-h-screen">
      <div className="max-w-[1280px] mx-auto flex flex-col gap-6">
        <h2 className="text-2xl font-extrabold tracking-tight text-fg">Workflow detail widgets</h2>
        <div className="grid grid-cols-3 gap-4">
          <JobCostWidget
            summary={{
              workflow_id: "nightly-research",
              workflow_name: "nightly-research",
              cost_summary: COST_SUMMARY,
            }}
            loading={false}
          />
          <HealthWidget runs={HEALTH_RUNS} />
          <div data-mesh="pulse" className="rounded-card p-6 border border-border-subtle text-[color:var(--color-ink-900)] flex flex-col min-h-[260px]">
            <div className="text-eyebrow !text-fg-tertiary mb-3">Pattern &middot; noir callout on mesh</div>
            <NoirCallout eyebrow="Total this month">
              <div className="text-display text-4xl num leading-none">$92.41</div>
              <div className="text-eyebrow mt-2">across 84 runs</div>
            </NoirCallout>
          </div>
        </div>
        <JobCostTrendWidget data={DAILY_BUCKETS} />
      </div>
    </div>
  ),
};
