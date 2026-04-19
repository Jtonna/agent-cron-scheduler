import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { TooltipProvider } from "@/components/ui/tooltip";
import { AllJobsPage } from "../AllJobsPage";
import type { Job } from "@/lib/types";

// ---- Mocks ----

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    name: "Test Job",
    schedule: "*/5 * * * *",
    execution: { type: "ShellCommand", value: "echo hello" },
    enabled: true,
    timezone: null,
    working_dir: null,
    env_vars: null,
    timeout_secs: 60,
    log_environment: false,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: "2024-01-01T00:00:00Z",
    updated_at: "2024-01-01T00:00:00Z",
    last_run_at: null,
    last_exit_code: null,
    next_run_at: null,
    ...overrides,
  };
}

const mockRefresh = vi.fn();
const mockTriggerJob = vi.fn().mockResolvedValue(undefined);
const mockEnableJob = vi.fn().mockResolvedValue(undefined);
const mockDisableJob = vi.fn().mockResolvedValue(undefined);
const mockDeleteJob = vi.fn().mockResolvedValue(undefined);

let mockJobs: Job[] = [];
let mockLoading = false;
let mockError: string | null = null;

vi.mock("@/hooks/useJobs", () => ({
  useJobs: () => ({
    jobs: mockJobs,
    loading: mockLoading,
    error: mockError,
    refresh: mockRefresh,
    triggerJob: mockTriggerJob,
    enableJob: mockEnableJob,
    disableJob: mockDisableJob,
    deleteJob: mockDeleteJob,
  }),
}));

vi.mock("@/hooks/useSSE", () => ({
  useSSEEvents: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {},
  getBaseUrl: vi.fn(() => ""),
}));

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/jobs"]}>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <TooltipProvider>
            <AllJobsPage />
          </TooltipProvider>
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("AllJobsPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockJobs = [];
    mockLoading = false;
    mockError = null;
  });

  it("renders the page heading", () => {
    renderPage();
    expect(screen.getByText("All Jobs")).toBeInTheDocument();
  });

  it("renders a Create Job link pointing to /jobs/create", () => {
    renderPage();
    const link = screen.getByRole("link", { name: /create job/i });
    expect(link).toHaveAttribute("href", "/jobs/create");
  });

  it("shows loading spinner when loading with no jobs", () => {
    mockLoading = true;
    mockJobs = [];
    renderPage();
    // The spinner is an ArrowPathIcon with animate-spin class
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("does not show loading spinner when there are already jobs", () => {
    mockLoading = true;
    mockJobs = [makeJob()];
    renderPage();
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).not.toBeInTheDocument();
  });

  it("renders the job table with jobs", () => {
    mockJobs = [
      makeJob({ id: "1", name: "Alpha Job" }),
      makeJob({ id: "2", name: "Beta Job" }),
    ];
    renderPage();
    expect(screen.getByText("Alpha Job")).toBeInTheDocument();
    expect(screen.getByText("Beta Job")).toBeInTheDocument();
  });

  it("shows an error banner with a retry button when error is set", async () => {
    const user = userEvent.setup();
    mockError = "Network error";
    renderPage();

    expect(screen.getByText("Network error")).toBeInTheDocument();
    const retryBtn = screen.getByRole("button", { name: /retry/i });
    expect(retryBtn).toBeInTheDocument();

    await user.click(retryBtn);
    expect(mockRefresh).toHaveBeenCalled();
  });

  it("does not show error banner when no error", () => {
    mockError = null;
    renderPage();
    expect(screen.queryByText(/retry/i)).not.toBeInTheDocument();
  });

  it("renders empty state message when no jobs exist", () => {
    mockJobs = [];
    renderPage();
    expect(
      screen.getByText("No jobs found. Create your first job to get started.")
    ).toBeInTheDocument();
  });
});
