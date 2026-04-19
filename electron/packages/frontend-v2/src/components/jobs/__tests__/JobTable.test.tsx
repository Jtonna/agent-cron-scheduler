import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import type { Job } from "@/lib/types";
import { TooltipProvider } from "@/components/ui/tooltip";
import { JobTable } from "../JobTable";

// Mock react-router-dom's Link so it renders without a full router context issue
vi.mock("react-router-dom", async () => {
  const actual = await vi.importActual<typeof import("react-router-dom")>(
    "react-router-dom"
  );
  return {
    ...actual,
  };
});

function makeJob(overrides: Partial<Job> = {}): Job {
  return {
    id: "job-1",
    name: "Alpha Job",
    schedule: "*/5 * * * *",
    execution: { type: "ShellCommand", value: "echo hello" },
    enabled: true,
    timezone: null,
    working_dir: null,
    env_vars: null,
    timeout_secs: 60,
    log_environment: false,
    allow_concurrent: false,
    pre_hook: null,
    post_hook: null,
    pre_hook_script_type: null,
    post_hook_script_type: null,
    created_at: "2024-01-01T00:00:00Z",
    updated_at: "2024-01-01T00:00:00Z",
    last_run_at: null,
    last_exit_code: null,
    next_run_at: null,
    ...overrides,
  };
}

const defaultProps = {
  onTrigger: vi.fn().mockResolvedValue(undefined),
  onKill: vi.fn().mockResolvedValue(undefined),
  onEnable: vi.fn().mockResolvedValue(undefined),
  onDisable: vi.fn().mockResolvedValue(undefined),
  onDelete: vi.fn().mockResolvedValue(undefined),
};

function renderTable(jobs: Job[], props = defaultProps) {
  return render(
    <MemoryRouter>
      <TooltipProvider>
        <JobTable jobs={jobs} {...props} />
      </TooltipProvider>
    </MemoryRouter>
  );
}

describe("JobTable", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders rows with job data", () => {
    const jobs = [
      makeJob({ id: "1", name: "Alpha Job", schedule: "*/5 * * * *", execution: { type: "ShellCommand", value: "echo hi" } }),
      makeJob({ id: "2", name: "Beta Job", schedule: "0 * * * *", execution: { type: "ScriptFile", value: "script.py" } }),
    ];
    renderTable(jobs);

    expect(screen.getByText("Alpha Job")).toBeInTheDocument();
    expect(screen.getByText("Beta Job")).toBeInTheDocument();
    expect(screen.getByText("*/5 * * * *")).toBeInTheDocument();
    expect(screen.getByText("0 * * * *")).toBeInTheDocument();
    expect(screen.getByText("Shell")).toBeInTheDocument();
    expect(screen.getByText("Script")).toBeInTheDocument();
  });

  it("renders empty state when no jobs", () => {
    renderTable([]);
    expect(
      screen.getByText("No jobs found. Create your first job to get started.")
    ).toBeInTheDocument();
  });

  it("links job names to /jobs/:id", () => {
    const jobs = [makeJob({ id: "abc-123", name: "My Job" })];
    renderTable(jobs);
    const link = screen.getByRole("link", { name: "My Job" });
    expect(link).toHaveAttribute("href", "/jobs/abc-123");
  });

  it("links edit icon to /jobs/:id/edit", () => {
    const jobs = [makeJob({ id: "abc-123", name: "My Job" })];
    renderTable(jobs);
    const editLinks = screen.getAllByRole("link");
    const editLink = editLinks.find((l) =>
      l.getAttribute("href")?.endsWith("/edit")
    );
    expect(editLink).toBeTruthy();
    expect(editLink).toHaveAttribute("href", "/jobs/abc-123/edit");
  });

  describe("sorting", () => {
    it("sorts by name ascending by default", () => {
      const jobs = [
        makeJob({ id: "1", name: "Zebra Job" }),
        makeJob({ id: "2", name: "Alpha Job" }),
        makeJob({ id: "3", name: "Middle Job" }),
      ];
      renderTable(jobs);

      const rows = screen.getAllByRole("row");
      // rows[0] is header, rows[1..] are data rows
      expect(within(rows[1]).getByText("Alpha Job")).toBeInTheDocument();
      expect(within(rows[2]).getByText("Middle Job")).toBeInTheDocument();
      expect(within(rows[3]).getByText("Zebra Job")).toBeInTheDocument();
    });

    it("clicking Name header toggles sort direction", async () => {
      const user = userEvent.setup();
      const jobs = [
        makeJob({ id: "1", name: "Zebra Job" }),
        makeJob({ id: "2", name: "Alpha Job" }),
      ];
      renderTable(jobs);

      // Initially ascending: Alpha first
      let rows = screen.getAllByRole("row");
      expect(within(rows[1]).getByText("Alpha Job")).toBeInTheDocument();
      expect(within(rows[2]).getByText("Zebra Job")).toBeInTheDocument();

      // Click Name header to toggle to descending
      const nameHeader = screen.getByRole("columnheader", { name: /name/i });
      await user.click(nameHeader);

      rows = screen.getAllByRole("row");
      expect(within(rows[1]).getByText("Zebra Job")).toBeInTheDocument();
      expect(within(rows[2]).getByText("Alpha Job")).toBeInTheDocument();
    });

    it("clicking Name header twice restores ascending order", async () => {
      const user = userEvent.setup();
      const jobs = [
        makeJob({ id: "1", name: "Zebra Job" }),
        makeJob({ id: "2", name: "Alpha Job" }),
      ];
      renderTable(jobs);

      const nameHeader = screen.getByRole("columnheader", { name: /name/i });
      await user.click(nameHeader); // now descending
      await user.click(nameHeader); // back to ascending

      const rows = screen.getAllByRole("row");
      expect(within(rows[1]).getByText("Alpha Job")).toBeInTheDocument();
      expect(within(rows[2]).getByText("Zebra Job")).toBeInTheDocument();
    });
  });

  describe("delete confirmation dialog", () => {
    it("opens dialog when trash button is clicked", async () => {
      const user = userEvent.setup();
      const jobs = [makeJob({ id: "1", name: "Test Job" })];
      renderTable(jobs);

      const trashBtn = screen.getByRole("button", { name: /delete test job/i });
      await user.click(trashBtn);

      const dialog = screen.getByRole("alertdialog");
      expect(dialog).toBeInTheDocument();
      expect(within(dialog).getByText(/Test Job/)).toBeInTheDocument();
    });

    it("closes dialog when Cancel is clicked", async () => {
      const user = userEvent.setup();
      const jobs = [makeJob({ id: "1", name: "Test Job" })];
      renderTable(jobs);

      await user.click(screen.getByRole("button", { name: /delete test job/i }));
      expect(screen.getByRole("alertdialog")).toBeInTheDocument();

      await user.click(screen.getByRole("button", { name: /cancel/i }));
      expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    });

    it("calls onDelete with correct id when confirmed", async () => {
      const user = userEvent.setup();
      const onDelete = vi.fn().mockResolvedValue(undefined);
      const jobs = [makeJob({ id: "job-42", name: "Delete Me" })];
      renderTable(jobs, { ...defaultProps, onDelete });

      // Open dialog
      await user.click(screen.getByRole("button", { name: /delete delete me/i }));

      // Confirm deletion — find the "Delete" button inside the dialog
      const dialog = screen.getByRole("alertdialog");
      const confirmBtn = within(dialog).getByRole("button", { name: /^delete$/i });
      await user.click(confirmBtn);

      expect(onDelete).toHaveBeenCalledWith("job-42");
      expect(onDelete).toHaveBeenCalledTimes(1);
    });
  });

  describe("action callbacks", () => {
    it("calls onTrigger with correct id for idle job", async () => {
      const user = userEvent.setup();
      const onTrigger = vi.fn().mockResolvedValue(undefined);
      // Idle job: last_exit_code is NOT null (completed), last_run_at set
      const jobs = [
        makeJob({ id: "job-1", name: "Idle Job", last_run_at: "2024-01-01T00:00:00Z", last_exit_code: 0 }),
      ];
      renderTable(jobs, { ...defaultProps, onTrigger });

      const triggerBtn = screen.getByRole("button", { name: "Trigger now" });
      await user.click(triggerBtn);

      expect(onTrigger).toHaveBeenCalledWith("job-1");
    });

    it("calls onKill with correct id for running job", async () => {
      const user = userEvent.setup();
      const onKill = vi.fn().mockResolvedValue(undefined);
      // Running job: last_exit_code is null but last_run_at is set
      const jobs = [
        makeJob({ id: "job-2", name: "Running Job", last_run_at: "2024-01-01T00:00:00Z", last_exit_code: null }),
      ];
      renderTable(jobs, { ...defaultProps, onKill });

      const killBtn = screen.getByRole("button", { name: "Kill job" });
      await user.click(killBtn);

      expect(onKill).toHaveBeenCalledWith("job-2");
    });

    it("calls onEnable with correct id when enabling disabled job", async () => {
      const user = userEvent.setup();
      const onEnable = vi.fn().mockResolvedValue(undefined);
      const jobs = [makeJob({ id: "job-3", name: "Disabled Job", enabled: false })];
      renderTable(jobs, { ...defaultProps, onEnable });

      const switchEl = screen.getByRole("switch", { name: /enable disabled job/i });
      await user.click(switchEl);

      expect(onEnable).toHaveBeenCalledWith("job-3");
    });

    it("calls onDisable with correct id when disabling enabled job", async () => {
      const user = userEvent.setup();
      const onDisable = vi.fn().mockResolvedValue(undefined);
      const jobs = [makeJob({ id: "job-4", name: "Enabled Job", enabled: true, last_exit_code: 0, last_run_at: "2024-01-01T00:00:00Z" })];
      renderTable(jobs, { ...defaultProps, onDisable });

      const switchEl = screen.getByRole("switch", { name: /disable enabled job/i });
      await user.click(switchEl);

      expect(onDisable).toHaveBeenCalledWith("job-4");
    });
  });

  describe("Next Run column", () => {
    it("shows '--' for disabled jobs", () => {
      const jobs = [
        makeJob({ id: "1", name: "Disabled", enabled: false, next_run_at: "2024-12-01T00:00:00Z" }),
      ];
      renderTable(jobs);
      expect(screen.getByText("--")).toBeInTheDocument();
    });
  });
});
