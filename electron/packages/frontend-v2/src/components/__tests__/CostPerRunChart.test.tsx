import { render, screen, fireEvent } from "@testing-library/react";
import { CostPerRunChart } from "../CostPerRunChart";
import type { DailyDataPoint } from "@/lib/types";

function makeDataPoint(overrides: Partial<DailyDataPoint> = {}): DailyDataPoint {
  return {
    date: "2024-01-15",
    runs: 5,
    cost: 0.025,
    input_tokens: 1000,
    output_tokens: 500,
    ...overrides,
  };
}

describe("CostPerRunChart", () => {
  const defaultProps = {
    data: [],
    loading: false,
    timeframe: "1W",
    onTimeframeChange: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows empty state when no data", () => {
    render(<CostPerRunChart {...defaultProps} />);
    expect(screen.getByText("No cost data available")).toBeInTheDocument();
  });

  it("renders timeframe buttons in empty state", () => {
    render(<CostPerRunChart {...defaultProps} />);
    expect(screen.getByRole("button", { name: "1W" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "2W" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1M" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "3M" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All" })).toBeInTheDocument();
  });

  it("shows loading state when loading is true", () => {
    render(<CostPerRunChart {...defaultProps} loading={true} />);
    expect(screen.getByText("Loading...")).toBeInTheDocument();
  });

  it("renders chart SVG with data", () => {
    const data = [
      makeDataPoint({ date: "2024-01-15", runs: 3, cost: 0.015 }),
      makeDataPoint({ date: "2024-01-16", runs: 5, cost: 0.025 }),
    ];
    render(<CostPerRunChart {...defaultProps} data={data} />);
    const chart = screen.getByRole("img", { name: /cost per run bar chart/i });
    expect(chart).toBeInTheDocument();
  });

  it("renders timeframe buttons when data is present", () => {
    const data = [makeDataPoint()];
    render(<CostPerRunChart {...defaultProps} data={data} />);
    expect(screen.getByRole("button", { name: "1W" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "1M" })).toBeInTheDocument();
  });

  it("calls onTimeframeChange when a timeframe button is clicked", () => {
    const onTimeframeChange = vi.fn();
    render(
      <CostPerRunChart
        {...defaultProps}
        onTimeframeChange={onTimeframeChange}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: "1M" }));
    expect(onTimeframeChange).toHaveBeenCalledWith("1M");
  });

  it("active timeframe button has different styling", () => {
    render(<CostPerRunChart {...defaultProps} timeframe="2W" />);
    const active = screen.getByRole("button", { name: "2W" });
    // The active button gets bg-accent class
    expect(active.className).toContain("bg-accent");
  });

  it("inactive timeframe buttons do not have bg-accent class", () => {
    render(<CostPerRunChart {...defaultProps} timeframe="1W" />);
    const inactive = screen.getByRole("button", { name: "1M" });
    expect(inactive.className).not.toContain("bg-accent");
  });

  it("renders bars for each data point in the SVG", () => {
    const data = [
      makeDataPoint({ date: "2024-01-15" }),
      makeDataPoint({ date: "2024-01-16" }),
      makeDataPoint({ date: "2024-01-17" }),
    ];
    render(<CostPerRunChart {...defaultProps} data={data} />);
    // Each data point renders a rect element
    const rects = document.querySelectorAll("rect");
    expect(rects.length).toBe(3);
  });

  it("handles data points with zero runs gracefully", () => {
    const data = [makeDataPoint({ runs: 0, cost: 0 })];
    render(<CostPerRunChart {...defaultProps} data={data} />);
    // Should render chart without crashing
    expect(screen.getByRole("img")).toBeInTheDocument();
  });
});
