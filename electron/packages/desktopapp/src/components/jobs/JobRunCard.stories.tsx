import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { JobRunCard, JobRun } from "./JobRunCard";
import { TabBar } from "@/components/ui/TabBar";

const meta: Meta<typeof JobRunCard> = {
  title: "Components/Jobs/JobRunCard",
  component: JobRunCard,
};
export default meta;

type Story = StoryObj<typeof JobRunCard>;

const MOCK_JOBS: JobRun[] = [
  { name: "backup-db", status: "running", duration: "0m 42s", timeAgo: "just now", cost: "0.003" },
  {
    name: "sync-users",
    status: "success",
    duration: "2m 14s",
    timeAgo: "2 min ago",
    cost: "0.042",
  },
  {
    name: "deploy-staging",
    status: "failed",
    duration: "1m 02s",
    timeAgo: "5 min ago",
    cost: "0.018",
  },
  { name: "health-check", status: "success", duration: "0m 08s", timeAgo: "3 min ago" },
  {
    name: "cleanup-logs",
    status: "warning",
    duration: "4m 31s",
    timeAgo: "8 min ago",
    cost: "0.091",
  },
  {
    name: "nightly-report",
    status: "success",
    duration: "12m 44s",
    timeAgo: "22 min ago",
    cost: "0.214",
  },
  { name: "index-rebuild", status: "failed", duration: "0m 03s", timeAgo: "30 min ago" },
  { name: "cert-renewal", status: "success", duration: "0m 22s", timeAgo: "1 hr ago" },
];

/* ── Single card stories ── */

export const Running: Story = {
  args: { job: MOCK_JOBS[0] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 300 }}>
        <Story />
      </div>
    ),
  ],
};

export const Success: Story = {
  args: { job: MOCK_JOBS[1] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 300 }}>
        <Story />
      </div>
    ),
  ],
};

export const Failed: Story = {
  args: { job: MOCK_JOBS[2] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 300 }}>
        <Story />
      </div>
    ),
  ],
};

export const Warning: Story = {
  args: { job: MOCK_JOBS[4] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 300 }}>
        <Story />
      </div>
    ),
  ],
};

export const NoCost: Story = {
  args: { job: MOCK_JOBS[3] },
  decorators: [
    (Story) => (
      <div style={{ maxWidth: 300 }}>
        <Story />
      </div>
    ),
  ],
  name: "Success — no cost",
};

/* ── Layout demos ── */

export const LayoutGrid4Col: StoryObj = {
  render: () => (
    <div>
      <TabBar tabs={["All runs", "Running", "Succeeded", "Failed"]} activeTab="All runs" />
      <div className="px-16 py-8 grid grid-cols-4 gap-4">
        {MOCK_JOBS.map((job, i) => (
          <JobRunCard key={i} job={job} />
        ))}
      </div>
    </div>
  ),
  name: "Layout — 4 column grid",
  parameters: { layout: "fullscreen" },
};

export const LayoutGrid3Col: StoryObj = {
  render: () => (
    <div>
      <TabBar tabs={["All runs", "Running", "Succeeded", "Failed"]} activeTab="All runs" />
      <div className="px-16 py-8 grid grid-cols-3 gap-4">
        {MOCK_JOBS.map((job, i) => (
          <JobRunCard key={i} job={job} />
        ))}
      </div>
    </div>
  ),
  name: "Layout — 3 column grid",
  parameters: { layout: "fullscreen" },
};

export const LayoutHorizontalScroll: StoryObj = {
  render: () => (
    <div>
      <TabBar tabs={["All runs", "Running", "Succeeded", "Failed"]} activeTab="All runs" />
      <div className="px-16 py-8 flex gap-4 overflow-x-auto">
        {MOCK_JOBS.map((job, i) => (
          <div key={i} className="min-w-[260px] shrink-0">
            <JobRunCard job={job} />
          </div>
        ))}
      </div>
    </div>
  ),
  name: "Layout — horizontal scroll",
  parameters: { layout: "fullscreen" },
};

export const LayoutList: StoryObj = {
  render: () => (
    <div>
      <TabBar tabs={["All runs", "Running", "Succeeded", "Failed"]} activeTab="All runs" />
      <div className="px-16 py-8 flex flex-col gap-2 max-w-2xl">
        {MOCK_JOBS.map((job, i) => (
          <JobRunCard key={i} job={job} />
        ))}
      </div>
    </div>
  ),
  name: "Layout — stacked list",
  parameters: { layout: "fullscreen" },
};
