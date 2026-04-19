import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SubNavTabs } from "../SubNavTabs";

const tabs = [
  { id: "overview", label: "Overview" },
  { id: "runs", label: "Runs", closable: true },
  { id: "logs", label: "Logs", closable: false },
];

describe("SubNavTabs", () => {
  it("renders provided tabs", () => {
    render(
      <SubNavTabs tabs={tabs} activeTab="overview" onTabChange={() => {}} />
    );
    expect(screen.getByText("Overview")).toBeInTheDocument();
    expect(screen.getByText("Runs")).toBeInTheDocument();
    expect(screen.getByText("Logs")).toBeInTheDocument();
  });

  it("highlights the active tab with aria-selected=true", () => {
    render(
      <SubNavTabs tabs={tabs} activeTab="runs" onTabChange={() => {}} />
    );
    const runsTab = screen.getByRole("tab", { name: /runs/i });
    expect(runsTab).toHaveAttribute("aria-selected", "true");

    const overviewTab = screen.getByRole("tab", { name: /overview/i });
    expect(overviewTab).toHaveAttribute("aria-selected", "false");
  });

  it("calls onTabChange when a tab is clicked", async () => {
    const handleChange = vi.fn();
    const user = userEvent.setup();
    render(
      <SubNavTabs tabs={tabs} activeTab="overview" onTabChange={handleChange} />
    );
    await user.click(screen.getByRole("tab", { name: /logs/i }));
    expect(handleChange).toHaveBeenCalledWith("logs");
  });

  it("shows close button only on closable tabs", () => {
    render(
      <SubNavTabs
        tabs={tabs}
        activeTab="overview"
        onTabChange={() => {}}
        onTabClose={() => {}}
      />
    );
    // "Runs" is closable — close button should exist
    expect(screen.getByRole("button", { name: /close runs/i })).toBeInTheDocument();
    // "Overview" is not closable — no close button
    expect(screen.queryByRole("button", { name: /close overview/i })).not.toBeInTheDocument();
    // "Logs" has closable: false — no close button
    expect(screen.queryByRole("button", { name: /close logs/i })).not.toBeInTheDocument();
  });

  it("calls onTabClose when close button is clicked", async () => {
    const handleClose = vi.fn();
    const handleChange = vi.fn();
    const user = userEvent.setup();
    render(
      <SubNavTabs
        tabs={tabs}
        activeTab="overview"
        onTabChange={handleChange}
        onTabClose={handleClose}
      />
    );
    await user.click(screen.getByRole("button", { name: /close runs/i }));
    expect(handleClose).toHaveBeenCalledWith("runs");
    // Should NOT also trigger tab change
    expect(handleChange).not.toHaveBeenCalled();
  });

  it("does not show close button when onTabClose is not provided", () => {
    render(
      <SubNavTabs tabs={tabs} activeTab="overview" onTabChange={() => {}} />
    );
    expect(screen.queryByRole("button", { name: /close/i })).not.toBeInTheDocument();
  });

  it("calls onTabChange on Enter key press", async () => {
    const handleChange = vi.fn();
    const user = userEvent.setup();
    render(
      <SubNavTabs tabs={tabs} activeTab="overview" onTabChange={handleChange} />
    );
    const runsTab = screen.getByRole("tab", { name: /runs/i });
    runsTab.focus();
    await user.keyboard("{Enter}");
    expect(handleChange).toHaveBeenCalledWith("runs");
  });
});
