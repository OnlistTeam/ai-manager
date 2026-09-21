import fs from "node:fs";
import path from "node:path";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Checkbox } from "@/shared/ui/Checkbox";

const CHECKBOX_TSX = path.resolve(
  __dirname,
  "..",
  "..",
  "..",
  "src",
  "shared",
  "ui",
  "Checkbox.tsx",
);

describe("Checkbox", () => {
  it("exposes a real checkbox role and state", () => {
    render(<Checkbox id="a" checked onCheckedChange={vi.fn()} />);
    expect(screen.getByRole("checkbox")).toBeChecked();
  });

  it("reports the new value on click and on Space", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Checkbox id="a" checked={false} onCheckedChange={onCheckedChange} />,
    );
    await userEvent.click(screen.getByRole("checkbox"));
    expect(onCheckedChange).toHaveBeenLastCalledWith(true);

    onCheckedChange.mockClear();
    screen.getByRole("checkbox").focus();
    await userEvent.keyboard(" ");
    expect(onCheckedChange).toHaveBeenLastCalledWith(true);
  });

  it("cannot be toggled while disabled", async () => {
    const onCheckedChange = vi.fn();
    render(
      <Checkbox id="a" checked disabled onCheckedChange={onCheckedChange} />,
    );
    await userEvent.click(screen.getByRole("checkbox"));
    expect(onCheckedChange).not.toHaveBeenCalled();
  });

  it("imports cn from the design system and uses tokens only", () => {
    const source = fs.readFileSync(CHECKBOX_TSX, "utf8");
    expect(source).toContain('from "./cn"');
    expect(source).not.toContain("@/lib/utils");
    expect(source).not.toMatch(/#[0-9a-fA-F]{3,8}\b/);
  });
});
