import { render, screen, fireEvent } from "@testing-library/react";
import { RunTable } from "../RunTable";
import type { JobRun } from "@/lib/types";

function makeRun(overrides: Partial<JobRun> = {}): JobRun {
  return {
    run_id: "aaaabbbb-cccc-dddd-eeee-ffff00001111",
    job_id: "job-1",
    started_at: "2024-01-15T10:00:00Z",
    finished_at: "2024-01-15T10:01:05Z",
    status: "Completed",
    exit_code: 0,
    log_size_bytes: 1024,
    error: null,
    duration_ms: 65000,
    ...overrides,
  };
}

describe("RunTable", () => {
  const defaultProps = {
    runs: [],
    totalCount: 0,
    loading: false,
    hasMore: false,
    onLoadMore: vi.fn(),
    onRunClick: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("shows empty state when no runs", () => {
    render(<RunTable {...defaultProps} />);
    expect(screen.getByText("No runs yet.")).toBeInTheDocument();
  });

  it("shows skeleton loader when loading with no runs", () => {
    render(<RunTable {...defaultProps} loading={true} />);
    // TableSkeleton renders pulse divs with animate-pulse
    const pulseDivs = document.querySelectorAll(".animate-pulse");
    expect(pulseDivs.length).toBeGreaterThan(0);
  });

  it("renders run rows with status badge and run ID", () => {
    const runs = [
      makeRun({ run_id: "aaaabbbb-cccc-dddd-eeee-ffff00001111", status: "Completed", exit_code: 0 }),
    ];
    render(<RunTable {...defaultProps} runs={runs} totalCount={1} />);
    // Run ID is sliced to 8 chars
    expect(screen.getByText("aaaabbbb")).toBeInTheDocument();
    // Status badge
    expect(screen.getByText("Completed")).toBeInTheDocument();
  });

  it("renders duration from duration_ms field", () => {
    const runs = [makeRun({ duration_ms: 65000 })];
    render(<RunTable {...defaultProps} runs={runs} totalCount={1} />);
    expect(screen.getByText("1m 5s")).toBeInTheDocument();
  });

  it("renders exit code for completed runs", () => {
    const runs = [makeRun({ exit_code: 1 })];
    render(<RunTable {...defaultProps} runs={runs} totalCount={1} />);
    expect(screen.getByText("1")).toBeInTheDocument();
  });

  it("renders '--' for exit code when null", () => {
    const runs = [makeRun({ exit_code: null, status: "Running", finished_at: null })];
    render(<RunTable {...defaultProps} runs={runs} totalCount={1} />);
    expect(screen.getByText("--")).toBeInTheDocument();
  });

  it("shows Load More button when hasMore is true", () => {
    const runs = [makeRun()];
    render(
      <RunTable
        {...defaultProps}
        runs={runs}
        totalCount={5}
        hasMore={true}
      />
    );
    expect(screen.getByRole("button", { name: /load more/i })).toBeInTheDocument();
  });

  it("calls onLoadMore when Load More is clicked", () => {
    const onLoadMore = vi.fn();
    const runs = [makeRun()];
    render(
      <RunTable
        {...defaultProps}
        runs={runs}
        totalCount={5}
        hasMore={true}
        onLoadMore={onLoadMore}
      />
    );
    fireEvent.click(screen.getByRole("button", { name: /load more/i }));
    expect(onLoadMore).toHaveBeenCalledTimes(1);
  });

  it("does not show Load More button when hasMore is false and not loading", () => {
    const runs = [makeRun()];
    render(
      <RunTable
        {...defaultProps}
        runs={runs}
        totalCount={1}
        hasMore={false}
      />
    );
    expect(screen.queryByRole("button", { name: /load more/i })).not.toBeInTheDocument();
  });

  it("calls onRunClick with run_id when a row is clicked", () => {
    const onRunClick = vi.fn();
    const runs = [makeRun({ run_id: "aaaabbbb-cccc-dddd-eeee-ffff00001111" })];
    render(
      <RunTable
        {...defaultProps}
        runs={runs}
        totalCount={1}
        onRunClick={onRunClick}
      />
    );
    // Click the run ID text cell
    fireEvent.click(screen.getByText("aaaabbbb"));
    expect(onRunClick).toHaveBeenCalledWith("aaaabbbb-cccc-dddd-eeee-ffff00001111");
  });

  it("shows CompletedWithWarnings label as 'Warnings'", () => {
    const runs = [makeRun({ status: "CompletedWithWarnings" })];
    render(<RunTable {...defaultProps} runs={runs} totalCount={1} />);
    expect(screen.getByText("Warnings")).toBeInTheDocument();
  });

  it("displays run count info when hasMore is true", () => {
    const runs = [makeRun(), makeRun({ run_id: "bbbbcccc-dddd-eeee-ffff-000011112222" })];
    render(
      <RunTable
        {...defaultProps}
        runs={runs}
        totalCount={10}
        hasMore={true}
      />
    );
    expect(screen.getByText(/Showing 2 of 10 runs/)).toBeInTheDocument();
  });
});
