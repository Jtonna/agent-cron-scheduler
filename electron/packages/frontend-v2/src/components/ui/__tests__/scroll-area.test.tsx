import { render, screen } from "@testing-library/react";
import { ScrollArea } from "../scroll-area";

describe("ScrollArea", () => {
  it("renders children content", () => {
    render(
      <ScrollArea>
        <p>Scrollable content</p>
      </ScrollArea>
    );
    expect(screen.getByText("Scrollable content")).toBeInTheDocument();
  });

  it("applies custom className", () => {
    render(
      <ScrollArea className="custom-scroll" data-testid="scroll">
        <p>Content</p>
      </ScrollArea>
    );
    expect(screen.getByTestId("scroll")).toHaveClass("custom-scroll");
  });
});
