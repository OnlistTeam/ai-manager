import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { ConfirmActionModal } from "@/features/tool-management";

const BASE = {
  open: true,
  title: "Install Claude Code?",
  description: "AI Manager will install everything required.",
  confirmLabel: "Install",
};

describe("ConfirmActionModal", () => {
  it("shows the plain-language question from spec section 31", () => {
    render(
      <ConfirmActionModal
        {...BASE}
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.getByText("Install Claude Code?")).toBeInTheDocument();
    expect(
      screen.getByText("AI Manager will install everything required."),
    ).toBeInTheDocument();
  });

  it("confirms and cancels through real buttons", async () => {
    const onConfirm = vi.fn();
    const onOpenChange = vi.fn();
    render(
      <ConfirmActionModal
        {...BASE}
        onOpenChange={onOpenChange}
        onConfirm={onConfirm}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "Install" }));
    expect(onConfirm).toHaveBeenCalledTimes(1);
    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.cancel" }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("starts on the affirmative action for a quick keyboard confirmation", () => {
    render(
      <ConfirmActionModal
        {...BASE}
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );

    expect(screen.getByRole("button", { name: "Install" })).toHaveFocus();
  });

  it("blocks both buttons while the request is in flight", () => {
    render(
      <ConfirmActionModal
        {...BASE}
        busy
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Install" })).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "ds.action.cancel" }),
    ).toBeDisabled();
  });

  it("renders nothing when closed", () => {
    render(
      <ConfirmActionModal
        {...BASE}
        open={false}
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.queryByText("Install Claude Code?")).not.toBeInTheDocument();
  });

  it("cannot be dismissed by Escape while the action is running", async () => {
    const onOpenChange = vi.fn();
    render(
      <ConfirmActionModal
        {...BASE}
        busy
        onOpenChange={onOpenChange}
        onConfirm={vi.fn()}
      />,
    );
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
