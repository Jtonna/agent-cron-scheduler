import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { WidgetCarousel } from "../WidgetCarousel";
import type { GlobalCostSummaryResponse, Job } from "@/lib/types";

// Mock chart child components with simple placeholders
vi.mock("../charts/DonutChart", () => ({
  DonutChart: () => <div data-testid="donut-chart">DonutChart</div>,
}));

vi.mock("../charts/BarChart", () => ({
  BarChart: () => <div data-testid="bar-chart">BarChart</div>,
}));

vi.mock("../charts/NextRunTimer", () => ({
  NextRunTimer: () => <div data-testid="next-run-timer">NextRunTimer</div>,
}));

const mockSummary: GlobalCostSummaryResponse = {
  timeframe: "30d",
  today_usd: 1.5,
  week_usd: 10.0,
  month_usd: 42.0,
  today_tokens: { input: 5000, output: 3000 },
  top_jobs: [
    { job_id: "1", job_name: "Job A", total_cost: 20, total_runs: 10 },
    { job_id: "2", job_name: "Job B", total_cost: 15, total_runs: 8 },
  ],
  daily_trend: [
    { date: "2026-04-18", cost_usd: 3.0, input_tokens: 1000, output_tokens: 800 },
    { date: "2026-04-19", cost_usd: 5.0, input_tokens: 2000, output_tokens: 1500 },
  ],
};

const mockJobs: Job[] = [
  {
    id: "j1",
    name: "Test Job",
    schedule: "*/5 * * * *",
    execution: { type: "ShellCommand", value: "echo hello" },
    enabled: true,
    timezone: null,
    working_dir: null,
    env_vars: null,
    timeout_secs: 300,
    log_environment: false,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: "2026-04-01T00:00:00Z",
    updated_at: "2026-04-01T00:00:00Z",
    last_run_at: null,
    last_exit_code: null,
    next_run_at: "2026-04-20T12:00:00Z",
  },
];

function renderCarousel(
  summary: GlobalCostSummaryResponse | null = mockSummary,
  jobs: Job[] = mockJobs
) {
  return render(
    <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
      <AccentThemeProvider>
        <WidgetCarousel summary={summary} jobs={jobs} />
      </AccentThemeProvider>
    </ThemeProvider>
  );
}

describe("WidgetCarousel", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders 3 dot indicators", () => {
    renderCarousel();
    const dots = screen.getByTestId("carousel-dots");
    const buttons = dots.querySelectorAll("button");
    expect(buttons).toHaveLength(3);
  });

  it("renders the first slide (DonutChart) by default", () => {
    renderCarousel();
    expect(screen.getByTestId("donut-chart")).toBeInTheDocument();
  });

  it("shows all 3 slides when navigating via dots", async () => {
    const user = userEvent.setup();
    renderCarousel();

    // Slide 1: DonutChart is already showing
    expect(screen.getByTestId("donut-chart")).toBeInTheDocument();

    // Click dot 2 -> BarChart
    const dots = screen.getByTestId("carousel-dots").querySelectorAll("button");
    await user.click(dots[1]);
    expect(screen.getByTestId("bar-chart")).toBeInTheDocument();

    // Click dot 3 -> NextRunTimer
    await user.click(dots[2]);
    expect(screen.getByTestId("next-run-timer")).toBeInTheDocument();
  });

  it("auto-advances with fake timers", () => {
    vi.useFakeTimers();

    renderCarousel();

    // Initially shows DonutChart
    expect(screen.getByTestId("donut-chart")).toBeInTheDocument();

    // Advance 5 seconds -> should move to BarChart
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByTestId("bar-chart")).toBeInTheDocument();

    // Advance another 5 seconds -> should move to NextRunTimer
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByTestId("next-run-timer")).toBeInTheDocument();

    // Advance another 5 seconds -> should wrap back to DonutChart
    act(() => {
      vi.advanceTimersByTime(5000);
    });
    expect(screen.getByTestId("donut-chart")).toBeInTheDocument();

    vi.useRealTimers();
  });

  it("handles null summary (loading state)", () => {
    renderCarousel(null);
    expect(screen.getByTestId("widget-carousel-loading")).toBeInTheDocument();
    expect(screen.getByText("Loading widgets...")).toBeInTheDocument();
  });
});
