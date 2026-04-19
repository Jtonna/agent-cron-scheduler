import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Switch } from "../switch";

describe("Switch", () => {
  it("renders unchecked by default", () => {
    render(<Switch aria-label="Toggle" />);
    const switchEl = screen.getByRole("switch");
    expect(switchEl).not.toBeChecked();
  });

  it("clicking toggles checked state", async () => {
    const user = userEvent.setup();
    render(<Switch aria-label="Toggle" />);
    const switchEl = screen.getByRole("switch");
    expect(switchEl).not.toBeChecked();
    await user.click(switchEl);
    expect(switchEl).toBeChecked();
  });

  it("disabled state", () => {
    render(<Switch aria-label="Toggle" disabled />);
    expect(screen.getByRole("switch")).toBeDisabled();
  });

  it("onCheckedChange fires", async () => {
    const handleChange = vi.fn();
    const user = userEvent.setup();
    render(<Switch aria-label="Toggle" onCheckedChange={handleChange} />);
    await user.click(screen.getByRole("switch"));
    expect(handleChange).toHaveBeenCalledWith(true);
  });
});
