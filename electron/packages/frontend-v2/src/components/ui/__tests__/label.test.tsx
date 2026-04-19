import { render, screen } from "@testing-library/react";
import { Label } from "../label";

describe("Label", () => {
  it("renders text content", () => {
    render(<Label>Email</Label>);
    expect(screen.getByText("Email")).toBeInTheDocument();
  });

  it("applies custom className", () => {
    render(<Label className="custom-class">Label</Label>);
    expect(screen.getByText("Label")).toHaveClass("custom-class");
  });

  it("htmlFor prop works", () => {
    render(<Label htmlFor="email-input">Email</Label>);
    expect(screen.getByText("Email")).toHaveAttribute("for", "email-input");
  });
});
