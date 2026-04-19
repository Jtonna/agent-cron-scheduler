import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import type { Job } from "@/lib/types";
import { JobForm } from "../JobForm";

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
vi.mock("../ScriptEditor", () => ({
  ScriptEditor: ({ value, onChange }: { value: string; onChange: (v: string) => void }) => (
    <textarea
      data-testid="script-editor"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}));

// Mock react-router-dom's useNavigate
const mockNavigate = vi.fn();
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>("react-router-dom");
  return {
    ...actual,
    useNavigate: () => mockNavigate,
  };
});

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    name: "Test Job",
    schedule: "*/5 * * * *",
    execution: { type: "ShellCommand", value: "echo hello" },
    enabled: true,
    timezone: "UTC",
    working_dir: "/tmp",
    env_vars: { FOO: "bar" },
    timeout_secs: 3600,
    log_environment: false,
    allow_concurrent: false,
    schedule_mode: "Cron",
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

function renderForm(props: Partial<React.ComponentProps<typeof JobForm>> = {}) {
  const defaultProps = {
    title: "Create Job",
    submitLabel: "Save",
    onSubmit: vi.fn().mockResolvedValue(undefined),
    ...props,
  };

  return render(
    <MemoryRouter>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <JobForm {...defaultProps} />
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("JobForm", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders with all basic sections visible", () => {
    renderForm();

    // Title
    expect(screen.getByText("Create Job")).toBeInTheDocument();

    // Job name input
    expect(screen.getByLabelText("Job Name")).toBeInTheDocument();

    // Schedule section (cron fields)
    expect(screen.getByLabelText("Minute")).toBeInTheDocument();

    // Tab triggers
    expect(screen.getByRole("tab", { name: "Basic" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Advanced" })).toBeInTheDocument();

    // Submit and Cancel buttons
    expect(screen.getByRole("button", { name: "Save" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();

    // Enabled toggle
    expect(screen.getByText("Enabled")).toBeInTheDocument();
  });

  it("prevents submit on empty required fields", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    renderForm({ onSubmit });

    // Clear the name field (it starts empty in create mode)
    const nameInput = screen.getByLabelText("Job Name");
    await user.clear(nameInput);

    // Submit
    const submitBtn = screen.getByRole("button", { name: "Save" });
    await user.click(submitBtn);

    // onSubmit should NOT have been called
    expect(onSubmit).not.toHaveBeenCalled();

    // Validation errors should appear
    expect(screen.getByText("Name is required")).toBeInTheDocument();
  });

  it("switches tabs when clicking Advanced", async () => {
    const user = userEvent.setup();
    renderForm();

    // Initially basic tab is active
    expect(screen.getByLabelText("Job Name")).toBeInTheDocument();

    // Click on Advanced tab
    const advancedTab = screen.getByRole("tab", { name: "Advanced" });
    await user.click(advancedTab);

    // Advanced-only fields should be visible
    expect(screen.getByLabelText("Working Directory")).toBeInTheDocument();
    expect(screen.getByLabelText("Timeout (seconds)")).toBeInTheDocument();
  });

  it("shows pre-populated data in edit mode", () => {
    const job = makeJob({
      name: "My Backup Job",
      execution: { type: "ShellCommand", value: "rsync -avz /" },
      schedule: "0 2 * * *",
    });
    renderForm({ job, title: "Edit Job" });

    // Title
    expect(screen.getByText("Edit Job")).toBeInTheDocument();

    // Name input populated
    const nameInput = screen.getByLabelText("Job Name") as HTMLInputElement;
    expect(nameInput.value).toBe("My Backup Job");
  });

  it("calls onSubmit with built data when form is valid", async () => {
    const user = userEvent.setup();
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const job = makeJob({
      name: "Valid Job",
      execution: { type: "ShellCommand", value: "echo test" },
    });
    renderForm({ job, onSubmit });

    const submitBtn = screen.getByRole("button", { name: "Save" });
    await user.click(submitBtn);

    expect(onSubmit).toHaveBeenCalledTimes(1);
    const submittedData = onSubmit.mock.calls[0][0];
    expect(submittedData.name).toBe("Valid Job");
    expect(submittedData.execution.value).toBe("echo test");
  });

  it("shows execution type tabs (ShellCommand / ScriptFile)", () => {
    renderForm();

    expect(screen.getByRole("tab", { name: "Shell Command" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Script File" })).toBeInTheDocument();
  });

  it("navigates back when Cancel is clicked", async () => {
    const user = userEvent.setup();
    renderForm();

    const cancelBtn = screen.getByRole("button", { name: "Cancel" });
    await user.click(cancelBtn);

    expect(mockNavigate).toHaveBeenCalledWith(-1);
  });
});
