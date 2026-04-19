import { render } from "@testing-library/react";
import { Separator } from "../separator";

describe("Separator", () => {
  it("renders horizontal by default", () => {
    const { container } = render(<Separator data-testid="sep" />);
    const sep = container.firstChild as HTMLElement;
    expect(sep).toHaveAttribute("data-orientation", "horizontal");
  });

  it("renders vertical with orientation prop", () => {
    const { container } = render(<Separator orientation="vertical" />);
    const sep = container.firstChild as HTMLElement;
    expect(sep).toHaveAttribute("data-orientation", "vertical");
  });

  it("applies custom className", () => {
    const { container } = render(<Separator className="custom-sep" />);
    const sep = container.firstChild as HTMLElement;
    expect(sep).toHaveClass("custom-sep");
  });
});
