import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { CopyButton } from "@/shared/ui/CopyButton";

function withClipboard(writeText: (value: string) => Promise<void>) {
  Object.defineProperty(navigator, "clipboard", {
    configurable: true,
    value: { writeText },
  });
}

describe("CopyButton", () => {
  it("writes the value and then reports that it copied", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    withClipboard(writeText);
    render(<CopyButton value="sk-live-1234" label="API key" />);

    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.copyNamed" }),
    );

    expect(writeText).toHaveBeenCalledWith("sk-live-1234");
    // The name changed while focus stays on the button, so a screen reader
    // announces the change — no need for a separate live region.
    expect(
      await screen.findByRole("button", { name: "ds.action.copied" }),
    ).toBeInTheDocument();
  });

  it("stays quiet when the clipboard refuses", async () => {
    withClipboard(vi.fn().mockRejectedValue(new Error("denied")));
    render(<CopyButton value="sk-live-1234" label="API key" />);

    await userEvent.click(
      screen.getByRole("button", { name: "ds.action.copyNamed" }),
    );

    // The value is already fully visible right next to it, so the user can
    // select and copy it themselves; showing an error toast here would be
    // more annoying than the failure itself.
    expect(
      screen.getByRole("button", { name: "ds.action.copyNamed" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "ds.action.copied" }),
    ).toBeNull();
  });
});
