import { render, screen, fireEvent } from "@testing-library/react";
import { CronInput } from "../CronInput";

describe("CronInput", () => {
  it("renders 5 fields from expression", () => {
    render(<CronInput value="*/5 2 15 6 1" onChange={() => {}} />);

    // Each field rendered by its aria-label
    expect(screen.getByRole("textbox", { name: "Minute" })).toHaveValue("*/5");
    expect(screen.getByRole("textbox", { name: "Hour" })).toHaveValue("2");
    expect(screen.getByRole("textbox", { name: "Day of Month" })).toHaveValue("15");
    expect(screen.getByRole("textbox", { name: "Month" })).toHaveValue("6");
    expect(screen.getByRole("textbox", { name: "Weekday" })).toHaveValue("1");
  });

  it("field change fires onChange with updated expression", () => {
    const handleChange = vi.fn();
    render(<CronInput value="* * * * *" onChange={handleChange} />);

    const minuteInput = screen.getByRole("textbox", { name: "Minute" });
    fireEvent.change(minuteInput, { target: { value: "30" } });

    expect(handleChange).toHaveBeenCalledWith("30 * * * *");
  });

  it("empty field on blur reverts to * and fires onChange", () => {
    const handleChange = vi.fn();
    render(<CronInput value="5 * * * *" onChange={handleChange} />);

    const minuteInput = screen.getByRole("textbox", { name: "Minute" });
    fireEvent.change(minuteInput, { target: { value: "" } });
    fireEvent.blur(minuteInput);

    expect(handleChange).toHaveBeenLastCalledWith("* * * * *");
    expect(minuteInput).toHaveValue("*");
  });

  it("preset click updates all fields and fires onChange", () => {
    const handleChange = vi.fn();
    render(<CronInput value="* * * * *" onChange={handleChange} />);

    const hourlyButton = screen.getByRole("button", { name: "Hourly" });
    fireEvent.click(hourlyButton);

    expect(handleChange).toHaveBeenCalledWith("0 * * * *");
    expect(screen.getByRole("textbox", { name: "Minute" })).toHaveValue("0");
    expect(screen.getByRole("textbox", { name: "Hour" })).toHaveValue("*");
  });

  it("shows valid description with green styling for valid expression", () => {
    render(<CronInput value="0 9 * * 1-5" onChange={() => {}} />);

    // The description panel has emerald border class when valid
    const descPanel = screen
      .getByText(/Runs at 09:00/i)
      .closest("div");
    expect(descPanel).toHaveClass("border-emerald-500/30");
  });

  it("shows invalid description with red styling for invalid expression", () => {
    render(<CronInput value="999 * * * *" onChange={() => {}} />);

    // The error message is displayed in the description span
    const errorSpan = screen.getByText(/is not valid/i);
    expect(errorSpan).toBeInTheDocument();

    // The description panel wrapping the error has destructive styling
    const descPanel = errorSpan.closest("div");
    expect(descPanel).toHaveClass("bg-destructive/10");
    expect(descPanel).toHaveClass("border-destructive/30");
  });

  it("renders all 7 preset buttons", () => {
    render(<CronInput value="* * * * *" onChange={() => {}} />);

    expect(screen.getByRole("button", { name: "Every 5 min" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Every 15 min" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Every 30 min" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Hourly" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Daily" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Weekly" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Monthly" })).toBeInTheDocument();
  });

  it("syncs fields when value prop changes externally", () => {
    const { rerender } = render(
      <CronInput value="* * * * *" onChange={() => {}} />
    );

    rerender(<CronInput value="0 0 1 * *" onChange={() => {}} />);

    expect(screen.getByRole("textbox", { name: "Minute" })).toHaveValue("0");
    expect(screen.getByRole("textbox", { name: "Hour" })).toHaveValue("0");
    expect(screen.getByRole("textbox", { name: "Day of Month" })).toHaveValue("1");
  });
});
