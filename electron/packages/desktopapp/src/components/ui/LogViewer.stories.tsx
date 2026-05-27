import type { Meta, StoryObj } from "@storybook/nextjs-vite";
import { Square } from "lucide-react";
import { LogViewer } from "./LogViewer";

const SAMPLE_LOG = [
  "2026-05-16T09:00:00Z INFO  acs::daemon starting daemon on 127.0.0.1:8377",
  "2026-05-16T09:00:00Z INFO  acs::scheduler loaded 12 workflows from store",
  "2026-05-16T09:00:01Z DEBUG acs::sse client connected (id=abc123)",
  "2026-05-16T09:00:02Z INFO  acs::run workflow=nightly-cleanup run=42 status=running",
  "2026-05-16T09:00:05Z WARN  acs::run workflow=nightly-cleanup step=2 retrying (attempt 2/3)",
  "2026-05-16T09:00:07Z INFO  acs::run workflow=nightly-cleanup run=42 status=success duration=6.3s",
  "2026-05-16T09:00:10Z ERROR acs::run workflow=broken-pipeline run=43 status=failed",
  "  caused by: connection refused (os error 111)",
  "2026-05-16T09:00:12Z INFO  acs::scheduler next tick in 30s",
  "2026-05-16T09:00:42Z INFO  acs::scheduler tick: 0 workflows ready",
].join("\n");

const meta: Meta<typeof LogViewer> = {
  title: "Components/UI/LogViewer",
  component: LogViewer,
  decorators: [
    (Story) => (
      <div style={{ height: 480 }}>
        <Story />
      </div>
    ),
  ],
};
export default meta;

type Story = StoryObj<typeof LogViewer>;

export const Default: Story = {
  args: {
    text: SAMPLE_LOG,
  },
};

export const Empty: Story = {
  args: {
    text: "",
  },
};

export const WithActions: Story = {
  args: {
    text: SAMPLE_LOG,
    actions: (
      <button
        type="button"
        aria-label="Kill run"
        className="inline-flex items-center gap-1.5 text-xs font-medium px-2 py-1 rounded-input border border-status-failed-border bg-status-failed-bg text-status-failed hover:bg-status-failed/10 cursor-pointer outline-none focus-visible:ring-2 focus-visible:ring-brand-ring"
      >
        <Square size={12} />
        Stop
      </button>
    ),
  },
};
