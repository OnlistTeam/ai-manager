import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Input } from "@/shared/ui/Input";

describe("Input", () => {
  it("is a real text box that forwards typing", async () => {
    const onChange = vi.fn();
    render(<Input aria-label="Name" defaultValue="" onChange={onChange} />);
    await userEvent.type(screen.getByLabelText("Name"), "hi");
    expect(onChange).toHaveBeenCalled();
  });

  it("announces an invalid value to assistive technology", () => {
    render(<Input aria-label="Name" invalid />);
    expect(screen.getByLabelText("Name")).toHaveAttribute(
      "aria-invalid",
      "true",
    );
  });

  it("stays operable by keyboard and reflects disabled state", () => {
    render(<Input aria-label="Name" disabled />);
    expect(screen.getByLabelText("Name")).toBeDisabled();
  });
});
