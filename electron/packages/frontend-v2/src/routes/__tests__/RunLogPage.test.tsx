import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { TooltipProvider } from "@/components/ui/tooltip";
import { RunLogPage } from "../RunLogPage";
import type { Job, JobRun } from "@/lib/types";

// Mock LogViewer to avoid complex DOM interactions
vi.mock("@/components/LogViewer", () => ({
  LogViewer: ({ content, loading }: { content: string; loading: boolean }) => (
    <div data-testid="log-viewer">
      {loading ? <span>Loading log...</span> : <pre>{content}</pre>}
    </div>
  ),
}));

vi.mock("sonner", () => ({
  toast: { success: vi.fn(), error: vi.fn(), info: vi.fn() },
}));

// ---- Mock hooks ----

let mockJob: Job | null = null;
vi.mock("@/hooks/useJob", () => ({
  useJob: () => ({
    job: mockJob,
    loading: false,
    error: null,
    refresh: vi.fn(),
  }),
}));

let mockLog = "";
let mockLogLoading = false;
let mockLogError: string | null = null;
vi.mock("@/hooks/useRunLog", () => ({
  useRunLog: () => ({
    log: mockLog,
    loading: mockLogLoading,
    error: mockLogError,
  }),
}));

vi.mock("@/hooks/useSSE", () => ({
  useSSEEvents: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    listRuns: vi.fn(),
    killRun: vi.fn(),
    triggerJob: vi.fn(),
  },
  ApiError: class ApiError extends Error {
    status: number;
    constructor(msg: string, status: number) {
      super(msg);
      this.status = status;
    }
  },
  getBaseUrl: vi.fn(() => ""),
}));

// ---- Factory ----

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    name: "My Job",
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

function makeRun(overrides: Partial<JobRun> = {}): JobRun {
  return {
    run_id: "aaaabbbb-cccc-dddd-eeee-ffff00001111",
    job_id: "job-1",
    started_at: "2024-01-15T10:00:00Z",
    finished_at: "2024-01-15T10:01:05Z",
    status: "Completed",
    exit_code: 0,
    log_size_bytes: 2048,
    error: null,
    duration_ms: 65000,
    ...overrides,
  };
}

const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom");
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

async function getApi() {
  const { api } = await import("@/lib/api");
  return api;
}

function renderPage(jobId = "job-1", runId = "aaaabbbb-cccc-dddd-eeee-ffff00001111") {
  return render(
    <MemoryRouter initialEntries={[`/jobs/${jobId}/runs/${runId}`]}>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <TooltipProvider>
            <Routes>
              <Route path="/jobs/:id/runs/:runId" element={<RunLogPage />} />
            </Routes>
          </TooltipProvider>
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("RunLogPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockJob = null;
    mockLog = "";
    mockLogLoading = false;
    mockLogError = null;
  });

  it("shows loading spinner while run metadata loads", async () => {
    const api = await getApi();
    // listRuns hangs forever to simulate loading
    vi.mocked(api.listRuns).mockReturnValue(new Promise(() => {}) as never);
    renderPage();
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("renders run metadata after fetching run", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Completed", exit_code: 0, duration_ms: 65000 });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob({ name: "My Job" });

    renderPage("job-1", "aaaabbbb-cccc-dddd-eeee-ffff00001111");

    await waitFor(() => {
      // Short run ID in breadcrumb
      expect(screen.getByText(/Run aaaabbbb/)).toBeInTheDocument();
    });
  });

  it("renders the log content via LogViewer", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Completed" });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockLog = "Hello from the log output";
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      expect(screen.getByTestId("log-viewer")).toBeInTheDocument();
      expect(screen.getByText("Hello from the log output")).toBeInTheDocument();
    });
  });

  it("shows the run status badge after loading", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Completed", exit_code: 0 });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      expect(screen.getByText("Completed")).toBeInTheDocument();
    });
  });

  it("shows Kill button for running jobs", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Running", finished_at: null, exit_code: null });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /kill/i })).toBeInTheDocument();
    });
  });

  it("does not show Kill button for completed jobs", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Completed", exit_code: 0 });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      // Wait for run to appear
      expect(screen.getByText("Completed")).toBeInTheDocument();
    });

    expect(screen.queryByRole("button", { name: /kill/i })).not.toBeInTheDocument();
  });

  it("shows Re-run button for completed jobs", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Completed", exit_code: 0 });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /re-run/i })).toBeInTheDocument();
    });
  });

  it("renders Back button to return to job detail", async () => {
    const api = await getApi();
    const run = makeRun({ status: "Completed" });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob();

    renderPage("job-1");

    await waitFor(() => {
      const backBtn = screen.getByRole("button", { name: /back/i });
      expect(backBtn).toBeInTheDocument();
    });
  });

  it("shows error panel when run not found", async () => {
    const api = await getApi();
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [], total: 0 } as never);
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      expect(screen.getByText("Run not found")).toBeInTheDocument();
    });
  });

  it("shows duration metadata when available", async () => {
    const api = await getApi();
    const run = makeRun({ duration_ms: 5000 });
    vi.mocked(api.listRuns).mockResolvedValueOnce({ runs: [run], total: 1 } as never);
    mockJob = makeJob();

    renderPage();

    await waitFor(() => {
      expect(screen.getByText("Duration")).toBeInTheDocument();
      expect(screen.getByText("5.0s")).toBeInTheDocument();
    });
  });

  it("calls api.killRun when Kill button clicked on running job", async () => {
    const user = userEvent.setup();
    const api = await getApi();
    const run = makeRun({ status: "Running", finished_at: null, exit_code: null });
    vi.mocked(api.listRuns).mockResolvedValue({ runs: [run], total: 1 } as never);
    vi.mocked(api.killRun).mockResolvedValueOnce(undefined as never);
    mockJob = makeJob();

    renderPage("job-1", run.run_id);

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /kill/i })).toBeInTheDocument();
    });

    await user.click(screen.getByRole("button", { name: /kill/i }));

    await waitFor(() => {
      expect(api.killRun).toHaveBeenCalledWith("job-1", run.run_id);
    });
  });
});
