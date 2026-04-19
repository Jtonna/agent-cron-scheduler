import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { TooltipProvider } from "@/components/ui/tooltip";
import { JobDetailPage } from "../JobDetailPage";
import type { Job, JobRun } from "@/lib/types";

// Mock ResizeObserver (required by Radix UI primitives in jsdom)
class MockResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}
Object.defineProperty(window, "ResizeObserver", {
  writable: true,
  value: MockResizeObserver,
});

// Mock ScriptEditor (CodeMirror won't work in jsdom)
vi.mock("@/components/jobs/ScriptEditor", () => ({
  ScriptEditor: ({ value, onChange }: { value: string; onChange: (v: string) => void }) => (
    <textarea
      data-testid="script-editor"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn() },
}));

// ---- Mock hooks ----

let mockJob: Job | null = null;
let mockJobLoading = false;
let mockJobError: string | null = null;
const mockRefreshJob = vi.fn();

vi.mock("@/hooks/useJob", () => ({
  useJob: () => ({
    job: mockJob,
    loading: mockJobLoading,
    error: mockJobError,
    refresh: mockRefreshJob,
  }),
}));

const mockLoadMore = vi.fn();
let mockRuns: JobRun[] = [];
let mockRunsTotal = 0;
let mockRunsLoading = false;

vi.mock("@/hooks/useRuns", () => ({
  useRuns: () => ({
    runs: mockRuns,
    totalCount: mockRunsTotal,
    loading: mockRunsLoading,
    loadMore: mockLoadMore,
  }),
}));

let mockCostSummary: { timeframe: string; data: []; } | null = null;
const mockSetTimeframe = vi.fn();

vi.mock("@/hooks/useCostSummary", () => ({
  useCostSummary: () => ({
    costSummary: mockCostSummary,
    loading: false,
    setTimeframe: mockSetTimeframe,
  }),
}));

vi.mock("@/hooks/useSSE", () => ({
  useSSEEvents: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    updateJob: vi.fn().mockResolvedValue({}),
    deleteJob: vi.fn().mockResolvedValue(undefined),
    triggerJob: vi.fn().mockResolvedValue({ run_id: "run-abc" }),
  },
  getBaseUrl: vi.fn(() => ""),
}));

// ---- Factory ----

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

function renderPage(jobId = "job-1") {
  return render(
    <MemoryRouter initialEntries={[`/jobs/${jobId}`]}>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <TooltipProvider>
            <Routes>
              <Route path="/jobs/:id" element={<JobDetailPage />} />
            </Routes>
          </TooltipProvider>
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("JobDetailPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockJob = null;
    mockJobLoading = false;
    mockJobError = null;
    mockRuns = [];
    mockRunsTotal = 0;
    mockRunsLoading = false;
    mockCostSummary = null;
  });

  it("shows loading spinner while job data loads", () => {
    mockJobLoading = true;
    mockJob = null;
    renderPage();
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("shows error state for missing or failed job", () => {
    mockJobError = "Job not found";
    mockJob = null;
    renderPage();
    expect(screen.getByText("Job not found")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /back to jobs/i })).toBeInTheDocument();
  });

  it("renders job details with job name in overview tab", () => {
    mockJob = makeJob({ name: "My Scheduled Job" });
    renderPage();
    expect(screen.getByText("My Scheduled Job")).toBeInTheDocument();
  });

  it("renders the schedule in the overview tab", () => {
    mockJob = makeJob({ schedule: "0 * * * *" });
    renderPage();
    expect(screen.getByText("0 * * * *")).toBeInTheDocument();
  });

  it("renders Overview and Configuration tab buttons", () => {
    mockJob = makeJob();
    renderPage();
    expect(screen.getByRole("tab", { name: "Overview" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Configuration" })).toBeInTheDocument();
  });

  it("switches to Configuration tab and renders the job form", async () => {
    const user = userEvent.setup();
    mockJob = makeJob({ name: "Tab Switch Job" });
    renderPage();

    const configTab = screen.getByRole("tab", { name: "Configuration" });
    await user.click(configTab);

    // The JobForm renders with the job title
    await waitFor(() => {
      expect(screen.getByText(/Edit: Tab Switch Job/)).toBeInTheDocument();
    });
  });

  it("renders Trigger Now button in overview", () => {
    mockJob = makeJob();
    renderPage();
    expect(screen.getByRole("button", { name: /trigger now/i })).toBeInTheDocument();
  });

  it("renders Delete button in overview", () => {
    mockJob = makeJob();
    renderPage();
    expect(screen.getByRole("button", { name: /^delete$/i })).toBeInTheDocument();
  });

  it("opens DeleteJobDialog when Delete button is clicked", async () => {
    const user = userEvent.setup();
    mockJob = makeJob({ name: "Deletable Job" });
    renderPage();

    // Find the destructive Delete button (not the dialog's "Delete Job" button)
    const deleteBtn = screen.getByRole("button", { name: /^delete$/i });
    await user.click(deleteBtn);

    // DeleteJobDialog title should appear with the "Delete Job" heading
    await waitFor(() => {
      // Dialog description mentions "permanently delete"
      expect(screen.getByText(/permanently delete/i)).toBeInTheDocument();
    });
  });

  it("shows Run History section in overview tab", () => {
    mockJob = makeJob();
    mockRuns = [];
    renderPage();
    expect(screen.getByText("Run History")).toBeInTheDocument();
  });

  it("shows Cost per Run section in overview tab", () => {
    mockJob = makeJob();
    renderPage();
    expect(screen.getByText("Cost per Run")).toBeInTheDocument();
  });

  it("shows View Script button in overview", () => {
    mockJob = makeJob();
    renderPage();
    expect(screen.getByRole("button", { name: /view script/i })).toBeInTheDocument();
  });
});
