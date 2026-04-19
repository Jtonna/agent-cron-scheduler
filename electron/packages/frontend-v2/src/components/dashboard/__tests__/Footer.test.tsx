import { render, screen } from "@testing-library/react";
import { ThemeProvider } from "next-themes";
import { AccentThemeProvider } from "@/hooks/use-accent-theme";
import { Footer } from "../Footer";
import type { HealthResponse } from "@/lib/types";

const sampleHealth: HealthResponse = {
  status: "ok",
  uptime_seconds: 3725, // 1h 2m 5s → should render "1h 2m"
  active_jobs: 3,
  total_jobs: 15,
  version: "1.4.2",
  data_dir: "/home/user/.local/share/acs",
};

function renderFooter(health: HealthResponse | null) {
  return render(
    <ThemeProvider attribute="class" defaultTheme="light" disableTransitionOnChange>
      <AccentThemeProvider>
        <Footer health={health} />
      </AccentThemeProvider>
    </ThemeProvider>
  );
}

describe("Footer", () => {
  it("renders all health fields when health is provided", () => {
    renderFooter(sampleHealth);

    // Uptime: 3725s = 1h 2m (days=0, hours=1, minutes=2)
    expect(screen.getByText(/up 1h 2m/)).toBeInTheDocument();

    // Active / total jobs
    expect(screen.getByText("3 / 15 jobs active")).toBeInTheDocument();

    // Version
    expect(screen.getByText("v1.4.2")).toBeInTheDocument();

    // Data directory
    expect(screen.getByText("/home/user/.local/share/acs")).toBeInTheDocument();
  });

  it("shows status dot with bg-success class when status is ok", () => {
    renderFooter(sampleHealth);
    const dot = screen.getByTestId("status-dot");
    expect(dot).toHaveClass("bg-success");
    expect(dot).not.toHaveClass("bg-destructive");
  });

  it("shows status dot with bg-destructive class when status is not ok", () => {
    const errorHealth: HealthResponse = {
      ...sampleHealth,
      status: "error",
    };
    renderFooter(errorHealth);
    const dot = screen.getByTestId("status-dot");
    expect(dot).toHaveClass("bg-destructive");
    expect(dot).not.toHaveClass("bg-success");
  });

  it("handles null health gracefully (loading state)", () => {
    renderFooter(null);

    // Status should show em-dash placeholder
    expect(screen.getByTestId("status-dot")).toBeInTheDocument();

    // Uptime placeholder
    expect(screen.getByText("up —")).toBeInTheDocument();

    // Jobs placeholder
    expect(screen.getByText("— / — jobs active")).toBeInTheDocument();

    // Version placeholder
    expect(screen.getByText("v—")).toBeInTheDocument();
  });

  it("renders the footer element", () => {
    renderFooter(sampleHealth);
    expect(screen.getByRole("contentinfo")).toBeInTheDocument();
  });
});
