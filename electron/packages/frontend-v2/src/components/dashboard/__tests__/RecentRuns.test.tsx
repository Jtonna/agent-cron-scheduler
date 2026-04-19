import { render, screen } from "@testing-library/react";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { MemoryRouter } from "react-router-dom";
import { vi, describe, it, expect, beforeEach } from "vitest";
import type { RecentRunEntry } from "@/lib/types";

vi.mock("@/lib/api", () => ({
  api: {
    listRecentRuns: vi.fn(),
  },
}));

import { api } from "@/lib/api";
import { RecentRuns } from "../RecentRuns";

const mockApi = api as { listRecentRuns: ReturnType<typeof vi.fn> };

const sampleRuns: RecentRunEntry[] = [
  {
    run_id: "run-1",
    job_id: "job-1",
    job_name: "Daily Backup",
    started_at: "2024-01-01T10:00:00Z",
    finished_at: "2024-01-01T10:05:00Z",
    status: "Completed",
    exit_code: 0,
    log_size_bytes: 1024,
    error: null,
    duration_ms: 83000,
    total_cost_usd: 0.0042,
    num_turns: null,
    model: null,
    usage: null,
  },
  {
    run_id: "run-2",
    job_id: "job-2",
    job_name: "Weekly Report",
    started_at: "2024-01-01T09:00:00Z",
    finished_at: "2024-01-01T09:10:00Z",
    status: "Failed",
    exit_code: 1,
    log_size_bytes: 512,
    error: "Command failed",
    duration_ms: 600000,
    total_cost_usd: null,
    num_turns: null,
    model: null,
    usage: null,
  },
  {
    run_id: "run-3",
    job_id: "job-3",
    job_name: "Running Task",
    started_at: "2024-01-02T08:00:00Z",
    finished_at: null,
    status: "Running",
    exit_code: null,
    log_size_bytes: 0,
    error: null,
    duration_ms: null,
    total_cost_usd: null,
    num_turns: null,
    model: null,
    usage: null,
  },
];

function renderRecentRuns() {
  return render(
    <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
      <AccentThemeProvider>
        <MemoryRouter>
          <RecentRuns />
        </MemoryRouter>
      </AccentThemeProvider>
    </ThemeProvider>
  );
}

describe("RecentRuns", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows loading skeleton initially", () => {
    // Keep the promise pending so loading remains true
    mockApi.listRecentRuns.mockReturnValue(new Promise(() => {}));
    renderRecentRuns();

    expect(screen.getByTestId("recent-runs-skeleton")).toBeInTheDocument();
  });

  it("renders run cards with correct data after loading", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: sampleRuns, limit: 12 });
    renderRecentRuns();

    // Wait for the grid to appear
    const grid = await screen.findByTestId("recent-runs-grid");
    expect(grid).toBeInTheDocument();

    // Job names
    expect(screen.getByText("Daily Backup")).toBeInTheDocument();
    expect(screen.getByText("Weekly Report")).toBeInTheDocument();
    expect(screen.getByText("Running Task")).toBeInTheDocument();

    // Status badges
    expect(screen.getByText("Completed")).toBeInTheDocument();
    expect(screen.getByText("Failed")).toBeInTheDocument();
    expect(screen.getByText("Running")).toBeInTheDocument();
  });

  it("displays duration and cost for run-1", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: sampleRuns, limit: 12 });
    renderRecentRuns();

    await screen.findByTestId("recent-runs-grid");

    // Duration: 83000ms = 1m 23s
    expect(screen.getByText("1m 23s")).toBeInTheDocument();

    // Cost: $0.0042
    expect(screen.getByText("$0.0042")).toBeInTheDocument();
  });

  it("shows '--' for null cost and duration", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: sampleRuns, limit: 12 });
    renderRecentRuns();

    await screen.findByTestId("recent-runs-grid");

    // run-2 and run-3 have null total_cost_usd — there should be at least one '--'
    const dashes = screen.getAllByText("--");
    expect(dashes.length).toBeGreaterThan(0);
  });

  it("shows empty state when no runs", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: [], limit: 12 });
    renderRecentRuns();

    expect(await screen.findByText("No recent runs")).toBeInTheDocument();
    expect(screen.queryByTestId("recent-runs-grid")).not.toBeInTheDocument();
  });

  it("shows error message when API fails", async () => {
    mockApi.listRecentRuns.mockRejectedValue(new Error("Network error"));
    renderRecentRuns();

    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("Network error");
  });

  it("renders the section heading", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: sampleRuns, limit: 12 });
    renderRecentRuns();

    // Wait for loading to complete so no act() warnings fire
    await screen.findByTestId("recent-runs-grid");
    expect(screen.getByText("Recent Runs")).toBeInTheDocument();
  });

  it("grid has responsive column classes", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: sampleRuns, limit: 12 });
    renderRecentRuns();

    const grid = await screen.findByTestId("recent-runs-grid");
    expect(grid).toHaveClass("grid-cols-1");
    expect(grid).toHaveClass("sm:grid-cols-2");
    expect(grid).toHaveClass("md:grid-cols-3");
    expect(grid).toHaveClass("xl:grid-cols-4");
  });

  it("renders correct number of run cards", async () => {
    mockApi.listRecentRuns.mockResolvedValue({ runs: sampleRuns, limit: 12 });
    renderRecentRuns();

    await screen.findByTestId("recent-runs-grid");

    // Each card has a job name — verify count via the presence of all 3
    const jobNames = ["Daily Backup", "Weekly Report", "Running Task"];
    jobNames.forEach((name) => {
      expect(screen.getByText(name)).toBeInTheDocument();
    });
  });
});
