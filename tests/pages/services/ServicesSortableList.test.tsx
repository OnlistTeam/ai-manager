import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Provider } from "@/entities/provider";
import en from "@/i18n/locales/en.json";
import { ServicesSortableList } from "@/pages/services/ServicesSortableList";

function service(id: string): Provider {
  return {
    id,
    tool: "claude-code",
    name: id,
    kind: "custom",
    active: false,
    baseUrl: null,
    apiKey: null,
    websiteUrl: null,
    testable: false,
    canRemove: true,
  };
}

const providers = [service("Alpha"), service("Beta"), service("Gamma")];

/**
 * jsdom lays nothing out, and the keyboard sensor moves between rows by their
 * rectangles. Give each row a stacked 100px slot based on its place among its
 * siblings, which is all the sortable keyboard coordinates need.
 */
function stackRows() {
  return vi
    .spyOn(Element.prototype, "getBoundingClientRect")
    .mockImplementation(function (this: Element) {
      const row = this.closest("[data-testid='row']") ?? this;
      const siblings = row.parentElement
        ? Array.from(row.parentElement.children)
        : [row];
      const top = Math.max(0, siblings.indexOf(row)) * 100;
      return {
        x: 0,
        y: top,
        top,
        left: 0,
        right: 400,
        bottom: top + 90,
        width: 400,
        height: 90,
        toJSON: () => ({}),
      } as DOMRect;
    });
}

function mount(
  onReorder: (ids: string[]) => void,
  options: { disabled?: boolean; items?: Provider[] } = {},
) {
  return render(
    <div>
      <ServicesSortableList
        providers={options.items ?? providers}
        disabled={options.disabled ?? false}
        onReorder={onReorder}
      >
        {(provider, handle) => (
          <article data-testid="row" aria-label={provider.name}>
            {provider.name}
            {handle}
          </article>
        )}
      </ServicesSortableList>
    </div>,
  );
}

describe("ServicesSortableList", () => {
  let rects: ReturnType<typeof stackRows>;

  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    rects = stackRows();
  });

  afterEach(() => {
    rects.mockRestore();
  });

  it("keeps the given order and names every handle after its service", () => {
    mount(vi.fn());
    expect(
      screen.getAllByRole("article").map((row) => row.textContent),
    ).toEqual(["Alpha", "Beta", "Gamma"]);
    expect(
      screen.getByRole("button", { name: "Reorder Beta" }),
    ).toHaveAttribute(
      "aria-roledescription",
      en.services.reorder.roleDescription,
    );
  });

  it("moves a service with the keyboard and reports the new order", async () => {
    const onReorder = vi.fn();
    const user = userEvent.setup();
    mount(onReorder);

    screen.getByRole("button", { name: "Reorder Alpha" }).focus();
    await user.keyboard(" ");
    await user.keyboard("{ArrowDown}");
    await user.keyboard(" ");

    await waitFor(() =>
      expect(onReorder).toHaveBeenCalledWith(["Beta", "Alpha", "Gamma"]),
    );
  });

  it("does not report anything when the move is cancelled", async () => {
    const onReorder = vi.fn();
    const user = userEvent.setup();
    mount(onReorder);

    screen.getByRole("button", { name: "Reorder Alpha" }).focus();
    await user.keyboard(" ");
    await user.keyboard("{ArrowDown}");
    await user.keyboard("{Escape}");

    expect(onReorder).not.toHaveBeenCalled();
  });

  it("offers no handle for a single service and a disabled one while busy", () => {
    const { unmount } = mount(vi.fn(), { items: [service("Alpha")] });
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
    unmount();

    mount(vi.fn(), { disabled: true });
    for (const handle of screen.getAllByRole("button")) {
      expect(handle).toBeDisabled();
    }
  });
});
