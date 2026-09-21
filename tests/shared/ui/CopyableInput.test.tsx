import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { CopyableInput } from "@/shared/ui/CopyableInput";
import { Field } from "@/shared/ui/Field";

describe("CopyableInput", () => {
  it("retains the input label and field hint", () => {
    render(
      <Field id="key" label="API key" hint="Saved locally">
        <CopyableInput id="key" value="test-key" copyLabel="API key" readOnly />
      </Field>,
    );
    expect(screen.getByLabelText("API key")).toHaveValue("test-key");
    expect(screen.getByLabelText("API key")).toHaveAccessibleDescription(
      "Saved locally",
    );
  });

  it("copies the latest value even when editing is disabled", async () => {
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    const { rerender } = render(
      <CopyableInput value="old" copyLabel="API key" disabled />,
    );
    rerender(<CopyableInput value="current" copyLabel="API key" disabled />);
    await user.click(screen.getByRole("button"));
    expect(writeText).toHaveBeenCalledWith("current");
  });

  it("does not offer to copy an empty value", () => {
    render(<CopyableInput value="" copyLabel="API key" readOnly />);
    expect(screen.queryByRole("button")).toBeNull();
  });
});
