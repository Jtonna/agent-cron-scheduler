import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { TopBar } from "../TopBar";

// Mock connections lib — no real localStorage dependency in these tests
vi.mock("@/lib/connections", () => ({
  getConnections: vi.fn(() => []),
  getActiveConnectionId: vi.fn(() => null),
  setActiveConnectionId: vi.fn(),
  deleteConnection: vi.fn(),
}));

// Mock the API to prevent real fetch calls
vi.mock("@/lib/api", () => ({
  api: {
    restart: vi.fn(),
    shutdown: vi.fn(),
  },
}));

// Mock InstanceDialog so we don't need its full dependency tree
vi.mock("@/components/InstanceDialog", () => ({
  InstanceDialog: () => null,
}));

function renderTopBar(initialPath = "/") {
  return render(
    <MemoryRouter initialEntries={[initialPath]}>
      <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
        <AccentThemeProvider>
          <TopBar />
        </AccentThemeProvider>
      </ThemeProvider>
    </MemoryRouter>
  );
}

describe("TopBar", () => {
  it("renders without crashing", () => {
    renderTopBar();
    expect(screen.getByRole("banner")).toBeInTheDocument();
  });

  it("shows 'Local' as the default instance name", () => {
    renderTopBar();
    expect(screen.getByText("Local")).toBeInTheDocument();
  });

  it("renders the PillToggle with Dashboard and Jobs options", () => {
    renderTopBar();
    expect(screen.getByText("Dashboard")).toBeInTheDocument();
    expect(screen.getByText("Jobs")).toBeInTheDocument();
  });

  it("renders the theme toggle button", () => {
    renderTopBar();
    // In light mode the button label is "Switch to dark mode"
    expect(
      screen.getByRole("button", { name: /switch to (dark|light) mode/i })
    ).toBeInTheDocument();
  });

  it("renders accent swatch buttons", () => {
    renderTopBar();
    expect(
      screen.getByRole("button", { name: /burgundy accent theme/i })
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /sage accent theme/i })
    ).toBeInTheDocument();
  });

  it("renders the settings gear button", () => {
    renderTopBar();
    expect(screen.getByRole("button", { name: /settings/i })).toBeInTheDocument();
  });

  it("highlights 'Jobs' pill when on /jobs route", () => {
    renderTopBar("/jobs");
    const jobsBtn = screen.getByText("Jobs");
    expect(jobsBtn).toHaveClass("bg-primary");
  });

  it("highlights 'Dashboard' pill when on / route", () => {
    renderTopBar("/");
    const dashboardBtn = screen.getByText("Dashboard");
    expect(dashboardBtn).toHaveClass("bg-primary");
  });
});
