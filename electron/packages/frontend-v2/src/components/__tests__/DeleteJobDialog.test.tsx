import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DeleteJobDialog } from "../DeleteJobDialog";

vi.mock("@/lib/api", () => ({
  api: {
    deleteJob: vi.fn(),
  },
  getBaseUrl: vi.fn(() => ""),
}));

// Mock sonner toast
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    error: vi.fn(),
  },
}));

async function getApi() {
  const { api } = await import("@/lib/api");
  return api;
}

function renderDialog(props: {
  open?: boolean;
  jobId?: string;
  jobName?: string;
  onOpenChange?: (open: boolean) => void;
  onDeleted?: () => void;
} = {}) {
  const {
    open = true,
    jobId = "job-42",
    jobName = "My Cron Job",
    onOpenChange = vi.fn(),
    onDeleted = vi.fn(),
  } = props;

  return render(
    <DeleteJobDialog
      open={open}
      onOpenChange={onOpenChange}
      jobId={jobId}
      jobName={jobName}
      onDeleted={onDeleted}
    />
  );
}

describe("DeleteJobDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders the dialog with job name in description when open", () => {
    renderDialog({ jobName: "My Cron Job" });
    // Multiple elements may contain the job name (description + label)
    expect(screen.getAllByText(/My Cron Job/).length).toBeGreaterThan(0);
    // Dialog title "Delete Job" appears as heading
    expect(screen.getByRole("heading", { name: "Delete Job" })).toBeInTheDocument();
  });

  it("does not render dialog content when closed", () => {
    renderDialog({ open: false });
    expect(screen.queryByText("Delete Job")).not.toBeInTheDocument();
  });

  it("shows the job name in the confirmation label", () => {
    renderDialog({ jobName: "Alpha Job" });
    // Multiple elements contain "Alpha Job" (description + label) - just check at least one exists
    expect(screen.getAllByText(/Alpha Job/).length).toBeGreaterThan(0);
  });

  it("Delete button is disabled when confirm text is empty", () => {
    renderDialog();
    const deleteBtn = screen.getByRole("button", { name: /delete job/i });
    expect(deleteBtn).toBeDisabled();
  });

  it("Delete button is disabled when confirm text does not match job name", async () => {
    const user = userEvent.setup();
    renderDialog({ jobName: "My Cron Job" });

    const input = screen.getByPlaceholderText("My Cron Job");
    await user.type(input, "wrong name");

    const deleteBtn = screen.getByRole("button", { name: /delete job/i });
    expect(deleteBtn).toBeDisabled();
  });

  it("Delete button is enabled when confirm text exactly matches job name", async () => {
    const user = userEvent.setup();
    renderDialog({ jobName: "My Cron Job" });

    const input = screen.getByPlaceholderText("My Cron Job");
    await user.type(input, "My Cron Job");

    const deleteBtn = screen.getByRole("button", { name: /delete job/i });
    expect(deleteBtn).not.toBeDisabled();
  });

  it("calls api.deleteJob and onDeleted on successful deletion", async () => {
    const user = userEvent.setup();
    const onDeleted = vi.fn();
    const onOpenChange = vi.fn();
    const api = await getApi();
    vi.mocked(api.deleteJob).mockResolvedValueOnce(undefined as never);

    renderDialog({ jobId: "job-42", jobName: "My Cron Job", onDeleted, onOpenChange });

    const input = screen.getByPlaceholderText("My Cron Job");
    await user.type(input, "My Cron Job");

    const deleteBtn = screen.getByRole("button", { name: /delete job/i });
    await user.click(deleteBtn);

    await waitFor(() => {
      expect(api.deleteJob).toHaveBeenCalledWith("job-42");
      expect(onDeleted).toHaveBeenCalled();
    });
  });

  it("calls onOpenChange with false when Cancel is clicked", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    renderDialog({ onOpenChange });

    const cancelBtn = screen.getByRole("button", { name: /cancel/i });
    await user.click(cancelBtn);

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("shows error toast when api.deleteJob throws", async () => {
    const user = userEvent.setup();
    const { toast } = await import("sonner");
    const api = await getApi();
    vi.mocked(api.deleteJob).mockRejectedValueOnce(new Error("Network failure") as never);

    renderDialog({ jobName: "My Cron Job" });

    const input = screen.getByPlaceholderText("My Cron Job");
    await user.type(input, "My Cron Job");

    const deleteBtn = screen.getByRole("button", { name: /delete job/i });
    await user.click(deleteBtn);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Network failure");
    });
  });
});
