import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import {
  Dialog,
  DialogTrigger,
  DialogContent,
  DialogTitle,
  DialogDescription,
} from "../dialog";

describe("Dialog", () => {
  it("trigger button opens dialog", async () => {
    const user = userEvent.setup();
    render(
      <Dialog>
        <DialogTrigger>Open</DialogTrigger>
        <DialogContent>
          <DialogTitle>Title</DialogTitle>
          <DialogDescription>Description</DialogDescription>
        </DialogContent>
      </Dialog>
    );
    expect(screen.queryByText("Title")).not.toBeInTheDocument();
    await user.click(screen.getByText("Open"));
    expect(screen.getByText("Title")).toBeInTheDocument();
  });

  it("renders DialogTitle and DialogDescription when open", () => {
    render(
      <Dialog open={true}>
        <DialogContent>
          <DialogTitle>My Title</DialogTitle>
          <DialogDescription>My Description</DialogDescription>
        </DialogContent>
      </Dialog>
    );
    expect(screen.getByText("My Title")).toBeInTheDocument();
    expect(screen.getByText("My Description")).toBeInTheDocument();
  });

  it("close button exists when dialog is open", () => {
    render(
      <Dialog open={true}>
        <DialogContent>
          <DialogTitle>Title</DialogTitle>
        </DialogContent>
      </Dialog>
    );
    expect(screen.getByText("Close")).toBeInTheDocument();
  });
});
