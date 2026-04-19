import { render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { EditJobPage } from "../EditJobPage";
import type { Job } from "@/lib/types";

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

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-42",
    name: "My Test Job",
    schedule: "0 * * * *",
    execution: { type: "ShellCommand", value: "echo test" },
    enabled: true,
    timezone: "UTC",
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

let mockJob: Job | null = null;
let mockLoading = false;
let mockError: string | null = null;

vi.mock("@/hooks/useJob", () => ({
  useJob: () => ({
    job: mockJob,
    loading: mockLoading,
    error: mockError,
    refresh: vi.fn(),
  }),
}));

const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom");
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

vi.mock("@/lib/api", () => ({
  api: {
    updateJob: vi.fn().mockResolvedValue({ id: "job-42" }),
  },
  getBaseUrl: vi.fn(() => ""),
}));

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/jobs/job-42/edit"]}>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <Routes>
            <Route path="/jobs/:id/edit" element={<EditJobPage />} />
          </Routes>
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("EditJobPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockJob = null;
    mockLoading = false;
    mockError = null;
  });

  it("shows a loading spinner while fetching", () => {
    mockLoading = true;
    renderPage();
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("shows an error message when fetch fails", () => {
    mockError = "Failed to fetch job";
    renderPage();
    expect(screen.getByText("Failed to fetch job")).toBeInTheDocument();
  });

  it("shows 'Job not found' when job is null and no error", () => {
    mockJob = null;
    mockLoading = false;
    mockError = null;
    renderPage();
    expect(screen.getByText("Job not found")).toBeInTheDocument();
  });

  it("renders the form with job data when loaded", () => {
    mockJob = makeJob({ name: "My Test Job" });
    renderPage();
    expect(screen.getByText("Edit Job: My Test Job")).toBeInTheDocument();
  });

  it("renders the Update Job submit button", () => {
    mockJob = makeJob();
    renderPage();
    expect(screen.getByRole("button", { name: "Update Job" })).toBeInTheDocument();
  });

  it("pre-populates the job name input", () => {
    mockJob = makeJob({ name: "Pre-filled Job" });
    renderPage();
    const nameInput = screen.getByLabelText("Job Name") as HTMLInputElement;
    expect(nameInput.value).toBe("Pre-filled Job");
  });
});
