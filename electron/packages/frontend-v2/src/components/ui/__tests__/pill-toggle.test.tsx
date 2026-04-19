import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { PillToggle } from "../pill-toggle";

const options = [
  { label: "Option A", value: "a" },
  { label: "Option B", value: "b" },
  { label: "Option C", value: "c" },
];

describe("PillToggle", () => {
  it("renders all options", () => {
    render(<PillToggle options={options} value="a" onChange={() => {}} />);
    expect(screen.getByText("Option A")).toBeInTheDocument();
    expect(screen.getByText("Option B")).toBeInTheDocument();
    expect(screen.getByText("Option C")).toBeInTheDocument();
  });

  it("shows active state on current value", () => {
    render(<PillToggle options={options} value="b" onChange={() => {}} />);
    const activeBtn = screen.getByText("Option B");
    expect(activeBtn).toHaveClass("bg-primary");
    const inactiveBtn = screen.getByText("Option A");
    expect(inactiveBtn).not.toHaveClass("bg-primary");
  });

  it("clicking an option calls onChange with that option's value", async () => {
    const handleChange = vi.fn();
    const user = userEvent.setup();
    render(<PillToggle options={options} value="a" onChange={handleChange} />);
    await user.click(screen.getByText("Option C"));
    expect(handleChange).toHaveBeenCalledWith("c");
  });

  it("applies custom className", () => {
    render(
      <PillToggle
        options={options}
        value="a"
        onChange={() => {}}
        className="custom-toggle"
        data-testid="toggle"
      />
    );
    // The className is applied to the outer div; check via data-testid won't work
    // since PillToggle uses forwardRef and applies className to the wrapper div
    const wrapper = screen.getByText("Option A").parentElement;
    expect(wrapper).toHaveClass("custom-toggle");
  });
});
