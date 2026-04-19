import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { HeroSection } from "../HeroSection";
import type { GlobalCostSummaryResponse, Job } from "@/lib/types";

// Mock child components with simple div placeholders
vi.mock("../ChatInput", () => ({
  ChatInput: () => <div data-testid="chat-input">ChatInput</div>,
}));

vi.mock("../QuickActions", () => ({
  QuickActions: ({ scrollToLogs }: { scrollToLogs?: () => void }) => (
    <div data-testid="quick-actions" data-has-scroll={!!scrollToLogs}>
      QuickActions
    </div>
  ),
}));

vi.mock("../WidgetCarousel", () => ({
  WidgetCarousel: () => <div data-testid="widget-carousel">WidgetCarousel</div>,
}));

const mockSummary: GlobalCostSummaryResponse = {
  timeframe: "30d",
  today_usd: 1.5,
  week_usd: 10.0,
  month_usd: 42.0,
  today_tokens: { input: 5000, output: 3000 },
  top_jobs: [
    { job_id: "1", job_name: "Job A", total_cost: 20, total_runs: 10 },
  ],
  daily_trend: [
    { date: "2026-04-18", cost_usd: 3.0, input_tokens: 1000, output_tokens: 800 },
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

function renderHeroSection(
  summary: GlobalCostSummaryResponse | null = mockSummary,
  jobs: Job[] = mockJobs,
  scrollToLogs?: () => void
) {
  return render(
    <MemoryRouter>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <HeroSection summary={summary} jobs={jobs} scrollToLogs={scrollToLogs} />
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("HeroSection", () => {
  it('renders heading "Dashboard"', () => {
    renderHeroSection();
    expect(screen.getByRole("heading", { name: "Dashboard" })).toBeInTheDocument();
  });

  it("renders subtitle text", () => {
    renderHeroSection();
    expect(screen.getByText("Monitor your jobs and system health")).toBeInTheDocument();
  });

  it("renders ChatInput mock", () => {
    renderHeroSection();
    expect(screen.getByTestId("chat-input")).toBeInTheDocument();
  });

  it("renders QuickActions mock", () => {
    renderHeroSection();
    expect(screen.getByTestId("quick-actions")).toBeInTheDocument();
  });

  it("renders WidgetCarousel mock", () => {
    renderHeroSection();
    expect(screen.getByTestId("widget-carousel")).toBeInTheDocument();
  });

  it("layout has correct grid classes", () => {
    renderHeroSection();
    const grid = screen.getByTestId("hero-section");
    expect(grid).toHaveClass("grid");
    expect(grid).toHaveClass("grid-cols-1");
    expect(grid).toHaveClass("gap-6");
  });
});
