import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useRef, useState } from "react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Modal } from "@/shared/ui/Modal";

/**
 * A click outside the dialog. Uses `fireEvent` rather than `userEvent`:
 * Radix sets `pointer-events: none` on the body while the dialog is open, so
 * user-event refuses to dispatch (it's the testing tool that gets blocked,
 * not a real user). `pointerdown` is exactly the event Radix's
 * `onInteractOutside` hooks into, so this exercises the actual wiring.
 */
function clickOutside() {
  fireEvent.pointerDown(document.body);
}

describe("Modal", () => {
  it("renders nothing while closed", () => {
    render(
      <Modal open={false} onOpenChange={vi.fn()} title="Remove Claude Code">
        Body
      </Modal>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("exposes an accessible dialog with a title and description", () => {
    render(
      <Modal
        open
        onOpenChange={vi.fn()}
        title="Remove Claude Code"
        description="This keeps your settings."
      >
        Body
      </Modal>,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog).toBeInTheDocument();
    expect(screen.getByText("Remove Claude Code")).toBeInTheDocument();
    expect(screen.getByText("This keeps your settings.")).toBeInTheDocument();
  });

  it("keeps a long user-owned target readable in the dialog title", () => {
    const title =
      "Remove SharedTeamProductionProviderConnectionUsedByEveryDesignerAndDeveloperInThisWholeOrganization?";
    render(
      <Modal open onOpenChange={vi.fn()} title={title}>
        Body
      </Modal>,
    );

    const heading = screen.getByRole("heading", { name: title });
    expect(heading).toHaveClass("break-words");
    expect(heading.parentElement).toHaveClass("min-w-0");
    expect(heading).not.toHaveAttribute("title");
  });

  it("closes on Escape and via the close button", async () => {
    const onOpenChange = vi.fn();
    render(
      <Modal open onOpenChange={onOpenChange} title="Remove Claude Code">
        Body
      </Modal>,
    );
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.close" }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
    onOpenChange.mockClear();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("renders the footer slot", () => {
    render(
      <Modal
        open
        onOpenChange={vi.fn()}
        title="Remove Claude Code"
        footer={<button type="button">Remove</button>}
      >
        Body
      </Modal>,
    );
    expect(screen.getByRole("button", { name: "Remove" })).toBeInTheDocument();
  });

  it("keeps the frame and footer visible while tall content scrolls", () => {
    render(
      <Modal
        open
        onOpenChange={vi.fn()}
        title="Connect a service"
        footer={<button type="button">Connect</button>}
      >
        <span>Scrollable connection fields</span>
      </Modal>,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog.className).toContain("max-h-[calc(100vh-2rem)]");
    expect(dialog.className).toContain("overflow-hidden");
    const scrollViewport = screen.getByText(
      "Scrollable connection fields",
    ).parentElement;
    expect(scrollViewport).toHaveClass("overflow-y-auto");
    expect(scrollViewport).toHaveClass("scrollbar-subtle");
    expect(
      screen.getByRole("button", { name: "Connect" }).parentElement?.className,
    ).toContain("shrink-0");
  });

  it("applies the size scale", () => {
    render(
      <Modal open onOpenChange={vi.fn()} title="t" size="lg">
        Body
      </Modal>,
    );
    expect(screen.getByRole("dialog").className).toContain("max-w-2xl");
  });

  it("uses the centered dialog motion instead of the anchored popover motion", () => {
    render(
      <Modal open onOpenChange={vi.fn()} title="t">
        Body
      </Modal>,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog.className).toContain("animate-ds-dialog-in");
    expect(dialog.className).not.toContain("animate-ds-modal-in");
  });

  it("stops Escape, the overlay and the close button while not dismissible", async () => {
    const onOpenChange = vi.fn();
    render(
      <Modal
        open
        dismissible={false}
        onOpenChange={onOpenChange}
        title="Remove Claude Code"
      >
        Body
      </Modal>,
    );

    expect(
      screen.queryByRole("button", { name: "ds.action.close" }),
    ).toBeNull();

    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();

    // A click outside the overlay is the third exit path, and the one most
    // easily missed in the implementation.
    clickOutside();
    expect(onOpenChange).not.toHaveBeenCalled();
  });

  it("keeps the close button, Escape and the overlay when dismissible is explicitly true", async () => {
    const onOpenChange = vi.fn();
    render(
      <Modal
        open
        dismissible
        onOpenChange={onOpenChange}
        title="Remove Claude Code"
      >
        Body
      </Modal>,
    );

    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.close" }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);

    onOpenChange.mockClear();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);

    onOpenChange.mockClear();
    clickOutside();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("still lets the caller close it programmatically while not dismissible", () => {
    const onOpenChange = vi.fn();
    const { rerender } = render(
      <Modal open dismissible={false} onOpenChange={onOpenChange} title="t">
        Body
      </Modal>,
    );
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    // `dismissible` only closes the three "user closes it themselves" paths;
    // it doesn't affect the caller closing it via `open`: dismissing the
    // dialog after a successful write goes through this path, and it must
    // not get locked out along with the rest.
    rerender(
      <Modal
        open={false}
        dismissible={false}
        onOpenChange={onOpenChange}
        title="t"
      >
        Body
      </Modal>,
    );
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("returns focus to a safe fallback when the opener becomes disabled", async () => {
    function Harness() {
      const [open, setOpen] = useState(false);
      const [blocked, setBlocked] = useState(false);
      const fallbackRef = useRef<HTMLHeadingElement>(null);

      return (
        <>
          <button
            type="button"
            disabled={blocked}
            onClick={() => setOpen(true)}
          >
            Open tool
          </button>
          <h2 ref={fallbackRef} tabIndex={-1}>
            Tasks
          </h2>
          <Modal
            open={open}
            onOpenChange={setOpen}
            returnFocusFallbackRef={fallbackRef}
            title="Open Codex"
            footer={
              <button
                type="button"
                onClick={() => {
                  setBlocked(true);
                  setOpen(false);
                }}
              >
                Cancel
              </button>
            }
          >
            Choose a project folder.
          </Modal>
        </>
      );
    }

    render(<Harness />);
    const opener = screen.getByRole("button", { name: "Open tool" });
    await userEvent.click(opener);
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(opener).toBeDisabled();
    await waitFor(() =>
      expect(screen.getByRole("heading", { name: "Tasks" })).toHaveFocus(),
    );
  });
});
