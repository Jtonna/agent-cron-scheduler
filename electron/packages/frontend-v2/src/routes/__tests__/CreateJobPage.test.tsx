import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { CreateJobPage } from "../CreateJobPage";

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
    createJob: vi.fn().mockResolvedValue({ id: "new-job-id", name: "New Job" }),
  },
  getBaseUrl: vi.fn(() => ""),
}));

function renderPage() {
  return render(
    <MemoryRouter initialEntries={["/jobs/create"]}>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <CreateJobPage />
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("CreateJobPage", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders a Back to Jobs link pointing to /jobs", () => {
    renderPage();
    const link = screen.getByRole("link", { name: /back to jobs/i });
    expect(link).toHaveAttribute("href", "/jobs");
  });

  it("renders the job form with Create New Job title", () => {
    renderPage();
    expect(screen.getByText("Create New Job")).toBeInTheDocument();
  });

  it("renders the Create Job submit button", () => {
    renderPage();
    expect(screen.getByRole("button", { name: "Create Job" })).toBeInTheDocument();
  });

  it("renders the job name input field", () => {
    renderPage();
    expect(screen.getByLabelText("Job Name")).toBeInTheDocument();
  });
});
