import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { OptionPicker, type OptionPickerProps } from "@/shared/ui/OptionPicker";

function mount(overrides: Partial<OptionPickerProps> = {}) {
  const props: OptionPickerProps = {
    label: "Endpoints",
    options: [
      { id: "a", label: "Alpha", detail: "model-a", checked: true },
      { id: "b", label: "Beta" },
    ],
    actions: [{ id: "manage", label: "Manage…", onSelect: vi.fn() }],
    filterPlaceholder: "Filter",
    noMatch: "Nothing matches",
    onSelect: vi.fn(),
    children: <button type="button">Open</button>,
    ...overrides,
  };
  render(<OptionPicker {...props} />);
  return props;
}

describe("OptionPicker", () => {
  beforeEach(() => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    });
  });

  it("marks the current option and lists the actions last", async () => {
    mount();
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    const listbox = await screen.findByRole("listbox", { name: "Endpoints" });
    const options = within(listbox).getAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      "Alphamodel-a",
      "Beta",
      "Manage…",
    ]);
    expect(options[0]).toHaveAttribute("aria-current", "true");
    expect(options[1]).not.toHaveAttribute("aria-current");
  });

  it("walks the list from the keyboard straight after opening", async () => {
    const props = mount();
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Open" }));
    await screen.findByRole("listbox");
    await user.keyboard("{ArrowDown}{Enter}");
    expect(props.onSelect).toHaveBeenCalledWith("b");
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
  });

  it("shows a current option that cannot be chosen at full strength, and skips it", async () => {
    const props = mount({
      options: [
        { id: "outside", label: "Outside", checked: true, disabled: true },
        { id: "b", label: "Beta" },
      ],
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Open" }));
    const listbox = await screen.findByRole("listbox");
    const current = within(listbox).getByRole("option", { name: "Outside" });
    expect(current).toHaveAttribute("aria-current", "true");
    expect(current).toHaveAttribute("aria-disabled", "true");
    expect(current).not.toHaveClass("opacity-50");
    await user.click(current);
    expect(props.onSelect).not.toHaveBeenCalled();
    // The keyboard lands on the first option that can be chosen.
    await user.keyboard("{Enter}");
    expect(props.onSelect).toHaveBeenCalledWith("b");
  });

  it("runs an action and closes", async () => {
    const onManage = vi.fn();
    mount({
      actions: [{ id: "manage", label: "Manage…", onSelect: onManage }],
    });
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    await userEvent.click(
      await screen.findByRole("option", { name: "Manage…" }),
    );
    expect(onManage).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
  });

  it("says so when there is nothing to choose", async () => {
    mount({ options: [], empty: "Nothing saved yet" });
    await userEvent.click(screen.getByRole("button", { name: "Open" }));
    expect(await screen.findByText("Nothing saved yet")).toBeVisible();
    expect(screen.getAllByRole("option")).toHaveLength(1);
  });

  it("filters a long list and keeps the actions", async () => {
    mount({
      filterAbove: 2,
      options: ["North", "South", "East"].map((label) => ({
        id: label.toLowerCase(),
        label,
      })),
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Open" }));
    const filter = await screen.findByPlaceholderText("Filter");
    await user.type(filter, "zzz");
    expect(screen.getByText("Nothing matches")).toBeVisible();
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(["Manage…"]);
    await user.clear(filter);
    await user.type(filter, "so");
    expect(
      screen.getAllByRole("option").map((option) => option.textContent),
    ).toEqual(["South", "Manage…"]);
  });
});
