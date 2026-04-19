import { render, screen } from "@testing-library/react";
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from "../collapsible";

describe("Collapsible", () => {
  // Smoke test: collapsible is a thin re-export of Radix Collapsible
  it("renders without crashing", () => {
    render(
      <Collapsible>
        <CollapsibleTrigger>Toggle</CollapsibleTrigger>
        <CollapsibleContent>Hidden content</CollapsibleContent>
      </Collapsible>
    );
    expect(screen.getByText("Toggle")).toBeInTheDocument();
  });
});
