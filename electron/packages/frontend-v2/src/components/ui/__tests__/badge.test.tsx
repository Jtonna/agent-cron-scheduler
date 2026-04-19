import { render, screen } from "@testing-library/react";
import { Badge, statusToBadgeVariant } from "../badge";

describe("Badge", () => {
  it("renders without crashing", () => {
    render(<Badge>Test Badge</Badge>);
    expect(screen.getByText("Test Badge")).toBeInTheDocument();
  });

  it("applies custom className", () => {
    render(<Badge className="custom-class">Badge</Badge>);
    expect(screen.getByText("Badge")).toHaveClass("custom-class");
  });

  it("renders default variant", () => {
    render(<Badge variant="default">Default</Badge>);
    expect(screen.getByText("Default")).toHaveClass("bg-primary");
  });

  it("renders secondary variant", () => {
    render(<Badge variant="secondary">Secondary</Badge>);
    expect(screen.getByText("Secondary")).toHaveClass("bg-secondary");
  });

  it("renders destructive variant", () => {
    render(<Badge variant="destructive">Destructive</Badge>);
    expect(screen.getByText("Destructive")).toHaveClass("bg-destructive");
  });

  it("renders outline variant", () => {
    render(<Badge variant="outline">Outline</Badge>);
    expect(screen.getByText("Outline")).toHaveClass("text-foreground");
  });

  it("renders success variant", () => {
    render(<Badge variant="success">Success</Badge>);
    expect(screen.getByText("Success")).toHaveClass("text-success");
  });

  it("renders error variant", () => {
    render(<Badge variant="error">Error</Badge>);
    expect(screen.getByText("Error")).toHaveClass("text-error");
  });

  it("renders running variant", () => {
    render(<Badge variant="running">Running</Badge>);
    expect(screen.getByText("Running")).toHaveClass("text-running");
  });

  it("renders warning variant", () => {
    render(<Badge variant="warning">Warning</Badge>);
    expect(screen.getByText("Warning")).toHaveClass("text-warning");
  });

  it("renders disabled variant", () => {
    render(<Badge variant="disabled">Disabled</Badge>);
    expect(screen.getByText("Disabled")).toHaveClass("bg-muted");
  });
});

describe("statusToBadgeVariant", () => {
  it("returns success for Completed with exit code 0", () => {
    expect(statusToBadgeVariant("Completed", 0)).toBe("success");
  });

  it("returns error for Completed with non-zero exit code", () => {
    expect(statusToBadgeVariant("Completed", 1)).toBe("error");
  });

  it("returns error for Failed", () => {
    expect(statusToBadgeVariant("Failed")).toBe("error");
  });

  it("returns running for Running", () => {
    expect(statusToBadgeVariant("Running")).toBe("running");
  });

  it("returns warning for Killed", () => {
    expect(statusToBadgeVariant("Killed")).toBe("warning");
  });

  it("returns warning for CompletedWithWarnings", () => {
    expect(statusToBadgeVariant("CompletedWithWarnings")).toBe("warning");
  });

  it("returns default for unknown status", () => {
    expect(statusToBadgeVariant("Unknown")).toBe("default");
  });
});
