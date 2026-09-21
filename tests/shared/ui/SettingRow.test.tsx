import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { SettingRow } from "@/shared/ui/SettingRow";

describe("SettingRow", () => {
  it("wires the label to the control so clicking it focuses the input", async () => {
    render(
      <SettingRow label="Start at login" controlId="start-at-login">
        <input id="start-at-login" type="checkbox" />
      </SettingRow>,
    );
    await userEvent.click(screen.getByText("Start at login"));
    expect(screen.getByRole("checkbox")).toBeChecked();
  });

  it("falls back to plain text when there is no control id", () => {
    const { container } = render(
      <SettingRow label="Version">
        <span>3.19.2</span>
      </SettingRow>,
    );
    expect(container.querySelector("label")).toBeNull();
    expect(screen.getByText("Version")).toBeInTheDocument();
  });

  it("renders the optional description below the label", () => {
    render(
      <SettingRow label="Start at login" description="Opens when you sign in">
        <span>off</span>
      </SettingRow>,
    );
    expect(screen.getByText("Opens when you sign in").className).toContain(
      "text-content-muted",
    );
  });
});
