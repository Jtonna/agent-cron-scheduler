import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { TriggerOverrideDialog } from "../TriggerOverrideDialog";

vi.mock("@/lib/api", () => ({
  api: {
    triggerJob: vi.fn(),
  },
  getBaseUrl: vi.fn(() => ""),
}));

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
  onTriggered?: (runId: string) => void;
} = {}) {
  const {
    open = true,
    jobId = "job-1",
    jobName = "My Job",
    onOpenChange = vi.fn(),
    onTriggered = vi.fn(),
  } = props;

  return render(
    <TriggerOverrideDialog
      open={open}
      onOpenChange={onOpenChange}
      jobId={jobId}
      jobName={jobName}
      onTriggered={onTriggered}
    />
  );
}

describe("TriggerOverrideDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders with job name in title", () => {
    renderDialog({ jobName: "My Test Job" });
    expect(screen.getByText("Trigger My Test Job")).toBeInTheDocument();
  });

  it("does not render content when closed", () => {
    renderDialog({ open: false });
    expect(screen.queryByText(/Trigger/)).not.toBeInTheDocument();
  });

  it("renders Command Arguments input", () => {
    renderDialog();
    expect(screen.getByLabelText(/command arguments/i)).toBeInTheDocument();
  });

  it("renders Standard Input textarea", () => {
    renderDialog();
    expect(screen.getByLabelText(/standard input/i)).toBeInTheDocument();
  });

  it("submits with empty overrides (no args/env/stdin)", async () => {
    const user = userEvent.setup();
    const onTriggered = vi.fn();
    const api = await getApi();
    vi.mocked(api.triggerJob).mockResolvedValueOnce({ run_id: "run-abc" } as never);

    renderDialog({ jobId: "job-1", onTriggered });

    const triggerBtn = screen.getByRole("button", { name: /^trigger$/i });
    await user.click(triggerBtn);

    await waitFor(() => {
      // Called with no params (undefined) since all fields are empty
      expect(api.triggerJob).toHaveBeenCalledWith("job-1", undefined);
      expect(onTriggered).toHaveBeenCalledWith("run-abc");
    });
  });

  it("submits with populated args and calls onTriggered", async () => {
    const user = userEvent.setup();
    const onTriggered = vi.fn();
    const api = await getApi();
    vi.mocked(api.triggerJob).mockResolvedValueOnce({ run_id: "run-xyz" } as never);

    renderDialog({ jobId: "job-2", onTriggered });

    const argsInput = screen.getByLabelText(/command arguments/i);
    await user.type(argsInput, "--verbose --output /tmp");

    const triggerBtn = screen.getByRole("button", { name: /^trigger$/i });
    await user.click(triggerBtn);

    await waitFor(() => {
      expect(api.triggerJob).toHaveBeenCalledWith("job-2", {
        args: "--verbose --output /tmp",
      });
      expect(onTriggered).toHaveBeenCalledWith("run-xyz");
    });
  });

  it("submits with stdin content", async () => {
    const user = userEvent.setup();
    const api = await getApi();
    vi.mocked(api.triggerJob).mockResolvedValueOnce({ run_id: "run-stdin" } as never);

    renderDialog({ jobId: "job-3" });

    const stdinInput = screen.getByLabelText(/standard input/i);
    await user.type(stdinInput, "hello world");

    const triggerBtn = screen.getByRole("button", { name: /^trigger$/i });
    await user.click(triggerBtn);

    await waitFor(() => {
      expect(api.triggerJob).toHaveBeenCalledWith("job-3", {
        input: "hello world",
      });
    });
  });

  it("can add an environment variable row", async () => {
    const user = userEvent.setup();
    renderDialog();

    const addBtn = screen.getByRole("button", { name: /add variable/i });
    await user.click(addBtn);

    // After clicking, env var inputs appear
    expect(screen.getByPlaceholderText("KEY")).toBeInTheDocument();
    expect(screen.getByPlaceholderText("value")).toBeInTheDocument();
  });

  it("can remove an added environment variable row", async () => {
    const user = userEvent.setup();
    renderDialog();

    // Add a row
    await user.click(screen.getByRole("button", { name: /add variable/i }));
    expect(screen.getByPlaceholderText("KEY")).toBeInTheDocument();

    // Remove it
    const removeBtn = screen.getByRole("button", { name: /remove variable/i });
    await user.click(removeBtn);

    expect(screen.queryByPlaceholderText("KEY")).not.toBeInTheDocument();
  });

  it("calls onOpenChange with false when Cancel is clicked", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    renderDialog({ onOpenChange });

    const cancelBtn = screen.getByRole("button", { name: /cancel/i });
    await user.click(cancelBtn);

    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("shows error toast when api.triggerJob throws", async () => {
    const user = userEvent.setup();
    const { toast } = await import("sonner");
    const api = await getApi();
    vi.mocked(api.triggerJob).mockRejectedValueOnce(new Error("Trigger failed") as never);

    renderDialog();

    const triggerBtn = screen.getByRole("button", { name: /^trigger$/i });
    await user.click(triggerBtn);

    await waitFor(() => {
      expect(toast.error).toHaveBeenCalledWith("Trigger failed");
    });
  });
});
