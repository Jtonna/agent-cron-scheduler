import { render, screen } from "@testing-library/react";
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from "../tooltip";

describe("Tooltip", () => {
  it("renders without crashing", () => {
    render(
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger>Hover me</TooltipTrigger>
          <TooltipContent>Tooltip text</TooltipContent>
        </Tooltip>
      </TooltipProvider>
    );
    expect(screen.getByText("Hover me")).toBeInTheDocument();
  });
});
