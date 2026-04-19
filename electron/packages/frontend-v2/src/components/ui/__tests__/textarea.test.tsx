import { render, screen } from "@testing-library/react";
import { Textarea } from "../textarea";

describe("Textarea", () => {
  it("renders placeholder", () => {
    render(<Textarea placeholder="Enter description" />);
    expect(screen.getByPlaceholderText("Enter description")).toBeInTheDocument();
  });

  it("disabled state", () => {
    render(<Textarea disabled data-testid="textarea" />);
    expect(screen.getByTestId("textarea")).toBeDisabled();
  });

  it("applies custom className", () => {
    render(<Textarea className="custom-class" data-testid="textarea" />);
    expect(screen.getByTestId("textarea")).toHaveClass("custom-class");
  });

  it("has rounded-lg class", () => {
    render(<Textarea data-testid="textarea" />);
    expect(screen.getByTestId("textarea")).toHaveClass("rounded-lg");
  });
});
