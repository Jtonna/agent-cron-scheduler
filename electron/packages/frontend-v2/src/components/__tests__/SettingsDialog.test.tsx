import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { SettingsDialog } from "../SettingsDialog";

// ── Mocks ────────────────────────────────────────────────────────────────────

vi.mock("@/lib/api", () => ({
  api: {
    restart: vi.fn(),
    shutdown: vi.fn(),
    health: vi.fn(),
  },
  getBaseUrl: vi.fn(() => "http://localhost:3000"),
}));

vi.mock("@/lib/connections", () => ({
  getActiveConnection: vi.fn(() => null),
  getConnections: vi.fn(() => []),
  getActiveConnectionId: vi.fn(() => null),
}));

vi.mock("@/hooks/useConnectionStatus", () => ({
  useConnectionStatus: vi.fn(() => "connected"),
}));

vi.mock("@/hooks/useHealth", () => ({
  useHealth: vi.fn(() => ({
    health: {
      version: "1.2.3",
      data_dir: "/tmp/acs-data",
      uptime_seconds: 3661,
    },
    loading: false,
    error: null,
    refresh: vi.fn(),
  })),
}));

// ── Helper ───────────────────────────────────────────────────────────────────

function renderDialog(open = true, onOpenChange = vi.fn()) {
  return {
    onOpenChange,
    ...render(
      <MemoryRouter>
        <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
          <AccentThemeProvider>
            <SettingsDialog open={open} onOpenChange={onOpenChange} />
          </AccentThemeProvider>
        </ThemeProvider>
      </MemoryRouter>
    ),
  };
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("SettingsDialog", () => {
  it("renders the Settings title when open", () => {
    renderDialog(true);
    expect(screen.getByText("Settings")).toBeInTheDocument();
  });

  it("does not render content when closed", () => {
    renderDialog(false);
    expect(screen.queryByText("Settings")).not.toBeInTheDocument();
  });

  it("renders all 4 tab triggers", () => {
    renderDialog();
    expect(screen.getByRole("tab", { name: /appearance/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /connection/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /system/i })).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /about/i })).toBeInTheDocument();
  });

  it("shows theme buttons on the Appearance tab", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Dark" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "System" })).toBeInTheDocument();
  });

  it("shows accent swatches on the Appearance tab", () => {
    renderDialog();
    expect(screen.getByRole("button", { name: /burgundy accent/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /sage accent/i })).toBeInTheDocument();
  });

  it("shows connection status info on the Connection tab", async () => {
    const user = userEvent.setup();
    renderDialog();

    await user.click(screen.getByRole("tab", { name: /connection/i }));

    expect(screen.getByText("Current Connection")).toBeInTheDocument();
    expect(screen.getByText("Connected")).toBeInTheDocument();
  });

  it("shows Restart and Shutdown buttons on the System tab", async () => {
    const user = userEvent.setup();
    renderDialog();

    await user.click(screen.getByRole("tab", { name: /system/i }));

    expect(screen.getByRole("button", { name: /restart/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /shutdown/i })).toBeInTheDocument();
  });

  it("shows version, data dir, and GitHub link on the About tab", async () => {
    const user = userEvent.setup();
    renderDialog();

    await user.click(screen.getByRole("tab", { name: /about/i }));

    expect(screen.getByText("1.2.3")).toBeInTheDocument();
    expect(screen.getByText("/tmp/acs-data")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: /github/i })).toHaveAttribute(
      "href",
      "https://github.com/Jtonna/agent-cron-scheduler"
    );
  });

  it("calls onOpenChange when the close button is clicked", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    renderDialog(true, onOpenChange);

    const closeButton = screen.getByRole("button", { name: /close/i });
    await user.click(closeButton);

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
