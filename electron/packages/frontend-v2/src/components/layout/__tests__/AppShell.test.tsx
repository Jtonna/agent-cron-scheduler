import { render, screen } from "@testing-library/react";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { AppShell } from "../AppShell";

// Mock all sub-components that make real network requests or need complex setup
vi.mock("@/lib/connections", () => ({
  getConnections: vi.fn(() => []),
  getActiveConnectionId: vi.fn(() => null),
  setActiveConnectionId: vi.fn(),
  deleteConnection: vi.fn(),
}));

vi.mock("@/lib/api", () => ({
  api: {
    listJobs: vi.fn(() => Promise.resolve([])),
    getJob: vi.fn(() => Promise.resolve(null)),
    listRuns: vi.fn(() => Promise.resolve({ runs: [] })),
    restart: vi.fn(),
    shutdown: vi.fn(),
  },
  getBaseUrl: vi.fn(() => "http://localhost:3000"),
}));

vi.mock("@/components/InstanceDialog", () => ({
  InstanceDialog: () => null,
}));

vi.mock("@/components/ConnectionBanner", () => ({
  ConnectionBanner: () => null,
}));

// Mock page-level routes so they render a simple placeholder
vi.mock("@/routes/MonitoringPage", () => ({
  MonitoringPage: () => <div data-testid="monitoring-page">Monitoring</div>,
}));
vi.mock("@/routes/AllJobsPage", () => ({
  AllJobsPage: () => <div data-testid="all-jobs-page">All Jobs</div>,
}));
vi.mock("@/routes/CreateJobPage", () => ({
  CreateJobPage: () => <div>Create Job</div>,
}));
vi.mock("@/routes/JobDetailPage", () => ({
  JobDetailPage: () => <div>Job Detail</div>,
}));
vi.mock("@/routes/EditJobPage", () => ({
  EditJobPage: () => <div>Edit Job</div>,
}));
vi.mock("@/routes/RunLogPage", () => ({
  RunLogPage: () => <div>Run Log</div>,
}));

// Mock EventSource (used by SSEProvider internally)
class MockEventSource {
  onopen: (() => void) | null = null;
  onmessage: ((e: MessageEvent) => void) | null = null;
  onerror: ((e: Event) => void) | null = null;
  addEventListener = vi.fn();
  close = vi.fn();
}
Object.defineProperty(window, "EventSource", {
  writable: true,
  value: MockEventSource,
});

describe("AppShell", () => {
  it("renders without crashing", () => {
    render(
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <AppShell />
        </AccentThemeProvider>
      </ThemeProvider>
    );
    expect(document.body).toBeTruthy();
  });

  it("shows the TopBar", () => {
    render(
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <AppShell />
        </AccentThemeProvider>
      </ThemeProvider>
    );
    // TopBar renders as a <header> element
    expect(screen.getByRole("banner")).toBeInTheDocument();
  });

  it("shows placeholder content (MonitoringPage at /)", () => {
    render(
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <AppShell />
        </AccentThemeProvider>
      </ThemeProvider>
    );
    expect(screen.getByTestId("monitoring-page")).toBeInTheDocument();
  });

  it("renders a main content area", () => {
    render(
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <AppShell />
        </AccentThemeProvider>
      </ThemeProvider>
    );
    expect(screen.getByRole("main")).toBeInTheDocument();
  });
});
