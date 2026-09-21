import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ExtensionCard } from "@/features/extension-management";
import type { Extension } from "@/entities/extension";
import { NativeError } from "@/native";

function extension(overrides: Partial<Extension> = {}): Extension {
  return {
    kind: "mcp",
    id: "filesystem",
    scope: { kind: "tool", id: "claude-code" },
    name: "Filesystem",
    description: "Reads and writes files you pick.",
    enabled: false,
    canDisable: true,
    management: "managed",
    ...overrides,
  };
}

function toggleFailureError(): NativeError {
  return new NativeError({
    code: "CONFIG_WRITE_FAILED",
    messageKey: "error.extension.toggleFailed",
    technicalMessage: "/private/tool.json token=do-not-show",
    remediation: "error.remediation.checkPermissions",
    contextId: null,
  });
}

describe("ExtensionCard", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, extensions: en.extensions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows the name, the description and an off switch", () => {
    render(<ExtensionCard extension={extension()} onToggle={vi.fn()} />);
    expect(
      screen.getByRole("heading", { name: "Filesystem" }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("Reads and writes files you pick."),
    ).toBeInTheDocument();
    const control = screen.getByRole("switch", {
      name: i18n.t("extensions.card.itemLabel", {
        name: "Filesystem",
        position: 1,
        total: 1,
      }),
    });
    expect(control).not.toBeChecked();
    // State isn't conveyed by color alone (spec §98): there's a word next to the toggle.
    expect(screen.getByText(en.extensions.card.off)).toBeInTheDocument();
    expect(screen.getByText(en.extensions.mcp.title)).toBeInTheDocument();
    expect(screen.getByRole("article", { name: "Filesystem" })).toHaveAttribute(
      "aria-busy",
      "false",
    );
  });

  it("emphasizes nested keyboard actions without making the whole card look clickable", () => {
    render(<ExtensionCard extension={extension()} onToggle={vi.fn()} />);
    const card = screen.getByRole("article", { name: "Filesystem" });
    expect(card).toHaveClass(
      "focus-within:border-brand/25",
      "focus-within:shadow-md",
    );
    expect(card).not.toHaveClass("hover:-translate-y-0.5");
    expect(card).not.toHaveClass("hover:border-brand/25");
    expect(card).not.toHaveClass("hover:shadow-md");
  });

  it("keeps an unbroken local description readable without hiding its control", async () => {
    const description =
      "https://docs.example.test/extensions/ordinaryuserworkflowmetadatawithoutspacesmustremaincompletelyreadableinsideeveryextensioncard";
    render(
      <ExtensionCard
        extension={extension({ description })}
        onToggle={vi.fn()}
      />,
    );

    const copy = screen.getByText(description);
    expect(copy).toHaveClass("break-words", "[overflow-wrap:anywhere]");
    expect(copy).not.toHaveClass("truncate");

    await userEvent.tab();
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Filesystem",
          position: 1,
          total: 1,
        }),
      }),
    ).toHaveFocus();
  });

  it("reports the next value when switched", async () => {
    const onToggle = vi.fn();
    render(
      <ExtensionCard
        extension={extension({ enabled: true })}
        onToggle={onToggle}
      />,
    );
    expect(screen.getByText(en.extensions.card.on)).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Filesystem",
          position: 1,
          total: 1,
        }),
      }),
    );
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("shows a named Skill update action only when an update is known", async () => {
    const onUpdate = vi.fn();
    render(
      <ExtensionCard
        extension={extension({ kind: "skill", name: "Code review" })}
        updateAvailable
        onUpdate={onUpdate}
        onToggle={vi.fn()}
      />,
    );
    expect(screen.getByText(en.extensions.card.updateAvailable)).toBeVisible();
    await userEvent.click(
      screen.getByRole("button", {
        name: "Update Code review, item 1 of 1",
      }),
    );
    expect(onUpdate).toHaveBeenCalledTimes(1);
  });

  it("keeps a failed disable action safe and retryable on the same switch", async () => {
    const onToggle = vi.fn();
    render(
      <ExtensionCard
        extension={extension({ enabled: true })}
        failure={{
          intendedEnabled: false,
          error: toggleFailureError(),
        }}
        onToggle={onToggle}
      />,
    );

    const alert = screen.getByRole("alert", {
      name: "Could not turn off Filesystem",
    });
    expect(alert).toHaveTextContent(en.error.extension.toggleFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkPermissions);
    expect(alert).not.toHaveTextContent("/private/tool.json");
    expect(alert).not.toHaveTextContent("do-not-show");
    await userEvent.click(
      screen.getByRole("switch", {
        name: "Try turning off Filesystem again, item 1 of 1",
      }),
    );
    expect(onToggle).toHaveBeenCalledWith(false);
  });

  it("turns a failed prompt Use action into a named retry button", async () => {
    const onToggle = vi.fn();
    render(
      <ExtensionCard
        extension={extension({
          kind: "prompt",
          id: "house-rules",
          name: "House rules",
          canDisable: false,
          enabled: true,
        })}
        failure={{
          intendedEnabled: true,
          error: toggleFailureError(),
        }}
        onToggle={onToggle}
      />,
    );

    const retry = screen.getByRole("button", {
      name: "Try turning on House rules again, item 1 of 1",
    });
    expect(retry).toHaveTextContent(en.extensions.card.retry);
    await userEvent.click(retry);
    expect(onToggle).toHaveBeenCalledWith(true);
  });

  it("says so plainly when an extension carries no description", () => {
    render(
      <ExtensionCard
        extension={extension({ description: null })}
        onToggle={vi.fn()}
      />,
    );
    expect(
      screen.getByText(en.extensions.card.noDescription),
    ).toBeInTheDocument();
  });

  it("labels a local detected Skill and offers no destructive controls", () => {
    const onToggle = vi.fn();
    const onRemove = vi.fn();
    render(
      <ExtensionCard
        extension={extension({
          kind: "skill",
          id: "mobile-app-release",
          name: "Mobile App Release",
          enabled: true,
          canDisable: false,
          management: "detected",
        })}
        onRemove={onRemove}
        onToggle={onToggle}
      />,
    );

    expect(screen.getByText("Found on this Mac")).toBeInTheDocument();
    expect(
      screen.getByText(en.extensions.card.detectedDescription),
    ).toBeInTheDocument();
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("offers a pick instead of a switch when the kind cannot be switched off", async () => {
    const onToggle = vi.fn();
    render(
      <ExtensionCard
        extension={extension({
          kind: "prompt",
          id: "house-rules",
          name: "House rules",
          canDisable: false,
        })}
        onToggle={onToggle}
      />,
    );
    expect(screen.queryByRole("switch")).toBeNull();
    await userEvent.click(
      screen.getByRole("button", {
        name: i18n.t("extensions.card.useNamed", {
          name: "House rules",
          position: 1,
          total: 1,
        }),
      }),
    );
    expect(onToggle).toHaveBeenCalledWith(true);
  });

  it("offers a named Prompt edit action without exposing its content", async () => {
    const onEdit = vi.fn();
    const { container } = render(
      <ExtensionCard
        extension={extension({
          kind: "prompt",
          id: "house-rules",
          name: "House rules",
          canDisable: false,
        })}
        onEdit={onEdit}
        onToggle={vi.fn()}
      />,
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: "Edit House rules, item 1 of 1",
      }),
    );
    expect(onEdit).toHaveBeenCalledOnce();
    expect(container).not.toHaveTextContent("content");
    expect(container.querySelector("textarea")).toBeNull();
  });

  it("shows the one in use as active, with nothing to click", () => {
    render(
      <ExtensionCard
        extension={extension({
          kind: "prompt",
          canDisable: false,
          enabled: true,
        })}
        onToggle={vi.fn()}
      />,
    );
    expect(screen.getByText(en.ds.service.nowActive)).toBeInTheDocument();
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByRole("button", { name: en.ds.action.use })).toBeNull();
  });

  it("blocks input while another write is running", async () => {
    const onToggle = vi.fn();
    render(<ExtensionCard extension={extension()} busy onToggle={onToggle} />);
    await userEvent.click(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Filesystem",
          position: 1,
          total: 1,
        }),
      }),
    );
    expect(onToggle).not.toHaveBeenCalled();
  });

  it("offers a position-aware remove action only when the page enables it", async () => {
    const onRemove = vi.fn();
    const { rerender } = render(
      <ExtensionCard extension={extension()} onToggle={vi.fn()} />,
    );
    expect(
      screen.queryByRole("button", { name: /Remove Filesystem/ }),
    ).toBeNull();

    rerender(
      <ExtensionCard
        extension={extension()}
        position={2}
        total={3}
        onRemove={onRemove}
        onToggle={vi.fn()}
      />,
    );
    const remove = screen.getByRole("button", {
      name: i18n.t("extensions.card.removeNamed", {
        name: "Filesystem",
        position: 2,
        total: 3,
      }),
    });
    await userEvent.click(remove);
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it("blocks removal while another write is running", async () => {
    const onRemove = vi.fn();
    render(
      <ExtensionCard
        extension={extension()}
        busy
        onRemove={onRemove}
        onToggle={vi.fn()}
      />,
    );
    const remove = screen.getByRole("button", {
      name: i18n.t("extensions.card.removeNamed", {
        name: "Filesystem",
        position: 1,
        total: 1,
      }),
    });
    expect(remove).toBeDisabled();
    await userEvent.click(remove);
    expect(onRemove).not.toHaveBeenCalled();
  });

  it("shows card-level progress while this extension is changing", () => {
    render(
      <ExtensionCard extension={extension()} busy pending onToggle={vi.fn()} />,
    );
    const card = screen.getByRole("article", { name: "Filesystem" });
    expect(card).toHaveAttribute("aria-busy", "true");
    expect(
      within(card).getByText(en.extensions.card.updating),
    ).toBeInTheDocument();
    const indicator = within(card).getByRole("status").querySelector("svg");
    expect(indicator).toHaveClass("motion-safe:animate-spin");
    expect(indicator?.classList.contains("animate-spin")).toBe(false);
    expect(within(card).getByRole("switch")).toBeDisabled();
  });

  it("gives repeated names a position-aware control name", () => {
    render(
      <ExtensionCard
        extension={extension()}
        position={2}
        total={3}
        onToggle={vi.fn()}
      />,
    );
    expect(
      screen.getByRole("switch", {
        name: i18n.t("extensions.card.itemLabel", {
          name: "Filesystem",
          position: 2,
          total: 3,
        }),
      }),
    ).toBeInTheDocument();
  });

  it("says which tools already carry a detected Skill", () => {
    const detected = extension({
      kind: "skill",
      id: "unity-cli",
      name: "Unity CLI",
      enabled: true,
      canDisable: false,
      management: "detected",
    });
    const { rerender } = render(
      <ExtensionCard
        extension={detected}
        presentIn={["Claude Code"]}
        onToggle={vi.fn()}
      />,
    );
    expect(
      screen.getByText(
        i18n.t("extensions.card.onlyIn", { tool: "Claude Code" }),
      ),
    ).toBeInTheDocument();

    rerender(
      <ExtensionCard
        extension={detected}
        presentIn={["Claude Code", "Codex CLI"]}
        onToggle={vi.fn()}
      />,
    );
    expect(
      screen.getByText(
        i18n.t("extensions.card.alsoIn", {
          tools: ["Claude Code", "Codex CLI"],
        }),
      ),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(en.extensions.card.detectedDescription),
    ).toBeNull();
  });

  it("expands a clipped description only when it is really clipped", async () => {
    // jsdom lays nothing out, so the clamp has to be simulated: a paragraph
    // whose content is taller than its box is exactly what `line-clamp-2`
    // produces in the real app.
    const clipped = { get: () => 96, configurable: true };
    const box = { get: () => 40, configurable: true };
    Object.defineProperty(HTMLElement.prototype, "scrollHeight", clipped);
    Object.defineProperty(HTMLElement.prototype, "clientHeight", box);
    try {
      const description =
        "Ready-made know-how your AI tools can follow for one specific job, written down once and reused everywhere.";
      render(
        <ExtensionCard
          extension={extension({ description })}
          onToggle={vi.fn()}
        />,
      );

      const copy = screen.getByText(description);
      expect(copy).toHaveClass("line-clamp-2");
      const expand = screen.getByRole("button", {
        name: en.extensions.card.expandDescription,
      });
      expect(expand).toHaveAttribute("aria-expanded", "false");
      expect(expand).toHaveAttribute("aria-controls", copy.id);

      await userEvent.click(expand);
      expect(copy).not.toHaveClass("line-clamp-2");
      expect(
        screen.getByRole("button", {
          name: en.extensions.card.collapseDescription,
        }),
      ).toHaveAttribute("aria-expanded", "true");
    } finally {
      Reflect.deleteProperty(HTMLElement.prototype, "scrollHeight");
      Reflect.deleteProperty(HTMLElement.prototype, "clientHeight");
    }
  });

  it("leaves a short description without an expand control", () => {
    render(<ExtensionCard extension={extension()} onToggle={vi.fn()} />);
    expect(
      screen.queryByRole("button", {
        name: en.extensions.card.expandDescription,
      }),
    ).toBeNull();
  });

  it("never renders the extension's own configuration", () => {
    const { container } = render(
      <ExtensionCard extension={extension()} onToggle={vi.fn()} />,
    );
    // Spec §36: the raw config must not appear in any form on the first screen.
    expect(container.querySelector("textarea")).toBeNull();
    expect(container.querySelector("pre")).toBeNull();
    expect(container.querySelector("code")).toBeNull();
  });
});
