import { render, screen, waitFor } from "@testing-library/react";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { SystemLogs } from "../SystemLogs";

// Mock the api module
vi.mock("@/lib/api", () => ({
  api: {
    getSystemLogs: vi.fn(() => Promise.resolve("")),
  },
  getBaseUrl: vi.fn(() => "http://localhost:3000"),
}));

vi.mock("@/lib/connections", () => ({
  getConnections: vi.fn(() => []),
  getActiveConnectionId: vi.fn(() => null),
  getActiveConnection: vi.fn(() => null),
  setActiveConnectionId: vi.fn(),
  deleteConnection: vi.fn(),
}));

import { api } from "@/lib/api";
const mockGetSystemLogs = api.getSystemLogs as ReturnType<typeof vi.fn>;

function renderWithProviders(ui: React.ReactElement) {
  return render(
    <ThemeProvider
      attribute="class"
      defaultTheme="light"
      disableTransitionOnChange
    >
      <AccentThemeProvider>{ui}</AccentThemeProvider>
    </ThemeProvider>
  );
}

describe("SystemLogs", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mockGetSystemLogs.mockResolvedValue("");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("renders with tail size selector tabs", async () => {
    renderWithProviders(<SystemLogs />);

    // Check for tail size tabs
    expect(screen.getByRole("tab", { name: "100" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "200" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "500" })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "1K" })).toBeInTheDocument();
  });

  it("renders auto-refresh toggle", async () => {
    renderWithProviders(<SystemLogs />);

    // The switch should render
    expect(screen.getByRole("switch")).toBeInTheDocument();
    // Label text
    expect(screen.getByText("Auto-refresh")).toBeInTheDocument();
  });

  it("displays log content when loaded", async () => {
    mockGetSystemLogs.mockResolvedValue(
      "2024-01-01 INFO Starting server\n2024-01-01 INFO Listening on port 3000"
    );

    renderWithProviders(<SystemLogs />);

    await waitFor(() => {
      expect(
        screen.getByText(/Starting server/)
      ).toBeInTheDocument();
    });

    expect(
      screen.getByText(/Listening on port 3000/)
    ).toBeInTheDocument();
  });

  it("shows loading spinner initially", () => {
    // Make the promise never resolve to keep loading state
    mockGetSystemLogs.mockReturnValue(new Promise(() => {}));

    renderWithProviders(<SystemLogs />);

    // The loading spinner should be visible (ArrowPathIcon with animate-spin)
    const spinner = document.querySelector(".animate-spin");
    expect(spinner).toBeInTheDocument();
  });

  it("handles error state", async () => {
    mockGetSystemLogs.mockRejectedValue(new Error("Connection refused"));

    renderWithProviders(<SystemLogs />);

    await waitFor(() => {
      expect(screen.getByText("Connection refused")).toBeInTheDocument();
    });
  });

  it("handles empty log content", async () => {
    mockGetSystemLogs.mockResolvedValue("");

    renderWithProviders(<SystemLogs />);

    await waitFor(() => {
      expect(screen.getByText("No output.")).toBeInTheDocument();
    });
  });

  it("renders the section heading", () => {
    renderWithProviders(<SystemLogs />);

    expect(screen.getByText("System Logs")).toBeInTheDocument();
  });

  it("has the system-logs id for scroll-to behavior", () => {
    renderWithProviders(<SystemLogs />);

    const section = document.getElementById("system-logs");
    expect(section).toBeInTheDocument();
  });
});
