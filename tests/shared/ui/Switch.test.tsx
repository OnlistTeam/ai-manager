import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Switch } from "@/shared/ui/Switch";

describe("Switch", () => {
  it("exposes an on/off control with an accessible name", () => {
    render(
      <>
        <p id="filesystem-help">Applies only to this computer.</p>
        <Switch
          aria-label="Filesystem"
          aria-describedby="filesystem-help"
          checked
          onCheckedChange={vi.fn()}
        />
      </>,
    );
    const control = screen.getByRole("switch", { name: "Filesystem" });
    expect(control).toBeInTheDocument();
    expect(control).toBeChecked();
    expect(control).toHaveAccessibleDescription(
      "Applies only to this computer.",
    );
  });

  it("reports the next value on click and on the keyboard", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Switch
        aria-label="Filesystem"
        checked={false}
        onCheckedChange={onCheckedChange}
      />,
    );
    const control = screen.getByRole("switch", { name: "Filesystem" });

    await userEvent.click(control);
    expect(onCheckedChange).toHaveBeenLastCalledWith(true);

    onCheckedChange.mockClear();
    control.focus();
    await userEvent.keyboard(" ");
    expect(onCheckedChange).toHaveBeenLastCalledWith(true);
  });

  it("ignores input while disabled", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Switch
        aria-label="Filesystem"
        checked={false}
        disabled
        onCheckedChange={onCheckedChange}
      />,
    );
    await userEvent.click(screen.getByRole("switch", { name: "Filesystem" }));
    expect(onCheckedChange).not.toHaveBeenCalled();
  });
});
