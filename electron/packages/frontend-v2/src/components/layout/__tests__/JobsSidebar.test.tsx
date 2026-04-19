import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { JobsSidebar } from "../JobsSidebar";

// Prevent real API calls
vi.mock("@/lib/api", () => ({
  api: {
    listJobs: vi.fn(() => Promise.resolve([])),
    getJob: vi.fn(() =>
      Promise.resolve({
        id: "job-1",
        name: "Test Job",
        schedule: "* * * * *",
        enabled: true,
        last_exit_code: null,
      })
    ),
    listRuns: vi.fn(() => Promise.resolve({ runs: [] })),
    triggerJob: vi.fn(),
    deleteJob: vi.fn(),
  },
}));

// Prevent real SSE connections
vi.mock("@/hooks/useSSE", () => ({
  useSSEEvents: vi.fn(),
  SSEProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

function renderInRoute(path: string, routePattern: string) {
  return render(
    <MemoryRouter initialEntries={[path]}>
      <Routes>
        <Route path={routePattern} element={<JobsSidebar />} />
      </Routes>
    </MemoryRouter>
  );
}

describe("JobsSidebar", () => {
  it("renders in list state on /jobs route", async () => {
    renderInRoute("/jobs", "/jobs");
    // The list view renders a header with "Jobs"
    await waitFor(() => expect(screen.getByText("Jobs")).toBeInTheDocument());
  });

  it("shows 'Jobs' heading and Create button in list state", async () => {
    renderInRoute("/jobs", "/jobs");
    await waitFor(() => {
      expect(screen.getByText("Jobs")).toBeInTheDocument();
      expect(screen.getByRole("link", { name: /create/i })).toBeInTheDocument();
    });
  });

  it("renders in detail state on /jobs/:id route", async () => {
    renderInRoute("/jobs/job-1", "/jobs/:id");
    // Detail view should show a back button
    expect(
      await screen.findByRole("button", { name: /back to jobs/i })
    ).toBeInTheDocument();
  });

  it("shows back button in detail state", async () => {
    renderInRoute("/jobs/job-1", "/jobs/:id");
    expect(
      await screen.findByRole("button", { name: /back to jobs/i })
    ).toBeInTheDocument();
  });

  it("does not show Create button in detail state", async () => {
    renderInRoute("/jobs/job-1", "/jobs/:id");
    // Wait for detail view to appear
    await screen.findByRole("button", { name: /back to jobs/i });
    expect(screen.queryByRole("link", { name: /create/i })).not.toBeInTheDocument();
  });
});
