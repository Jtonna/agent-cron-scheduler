import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { apiStatusToJobState, JobStateIndicator } from "./JobStateIndicator";

describe("apiStatusToJobState", () => {
  it("maps Completed to success", () => {
    expect(apiStatusToJobState("Completed")).toBe("success");
  });

  it("maps Running to running", () => {
    expect(apiStatusToJobState("Running")).toBe("running");
  });

  it("maps CompletedWithWarnings to warning", () => {
    expect(apiStatusToJobState("CompletedWithWarnings")).toBe("warning");
  });

  it("maps Failed to failed", () => {
    expect(apiStatusToJobState("Failed")).toBe("failed");
  });

  it("maps Killed to killed", () => {
    expect(apiStatusToJobState("Killed")).toBe("killed");
  });
});

describe("JobStateIndicator", () => {
  it("renders dot variant with aria-label", () => {
    const { container } = render(<JobStateIndicator state="success" variant="dot" />);
    // Default state="success" label is "Succeeded"
    const dot = screen.getByLabelText("Succeeded");
    expect(dot).toBeInTheDocument();
    // The dot uses the bg-status-success-dot class.
    expect(dot.className).toContain("bg-status-success-dot");
    // Container should have a single rendered element.
    expect(container.firstChild).toBe(dot);
  });

  it("renders badge variant with label text and an icon", () => {
    const { container } = render(<JobStateIndicator state="success" variant="badge" />);
    expect(screen.getByText("Succeeded")).toBeInTheDocument();
    // Lucide icons render as <svg>; the badge always has a non-null icon for success.
    const svg = container.querySelector("svg");
    expect(svg).not.toBeNull();
  });

  it("renders label variant with the state's label text", () => {
    render(<JobStateIndicator state="failed" variant="label" />);
    expect(screen.getByText("Failed")).toBeInTheDocument();
  });

  it("applies animate-pulse when state is running", () => {
    render(<JobStateIndicator state="running" variant="dot" />);
    const dot = screen.getByLabelText("Running");
    expect(dot.className).toContain("animate-pulse");
  });

  it("uses a custom label when provided", () => {
    render(<JobStateIndicator state="success" variant="label" label="All good" />);
    expect(screen.getByText("All good")).toBeInTheDocument();
  });
});
