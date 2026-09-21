import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { UninstallModal } from "@/features/tool-management";
import type { Tool } from "@/entities/tool";

const PREVIEW = vi.hoisted(() => ({
  tool: "claude-code" as const,
  app: [
    {
      kind: "command" as const,
      value: "npm uninstall -g @anthropic-ai/claude-code",
      canRemoveAutomatically: true,
    },
  ],
  settings: [
    {
      kind: "directory" as const,
      value: "/Users/test/.claude",
      canRemoveAutomatically: true,
    },
  ],
  cache: [
    {
      kind: "directory" as const,
      value: "/Users/test/.claude/projects",
      canRemoveAutomatically: true,
    },
  ],
}));

vi.mock("@/entities/tool", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/entities/tool")>()),
  useToolUninstallPreview: () => ({
    data: PREVIEW,
    error: null,
    isPending: false,
    isSuccess: true,
  }),
}));

const TOOL: Tool = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "tool.claude-code.description",
  status: "installed",
  version: "2.3.1",
  latestVersion: null,
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: false,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: true,
    canManageVersion: false,
    canLaunch: true,
  },
  sessionsInsideSettings: true,
  environment: null,
};

const TOOL_WITHOUT_NESTED_SESSIONS: Tool = {
  ...TOOL,
  id: "opencode",
  name: "OpenCode",
  sessionsInsideSettings: false,
};

function open(onConfirm = vi.fn()) {
  render(
    <UninstallModal tool={TOOL} onOpenChange={vi.fn()} onConfirm={onConfirm} />,
  );
  return onConfirm;
}

describe("UninstallModal", () => {
  beforeEach(async () => {
    PREVIEW.app = [
      {
        kind: "command",
        value: "npm uninstall -g @anthropic-ai/claude-code",
        canRemoveAutomatically: true,
      },
    ];
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, tools: en.tools },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("offers the three options with only the app checked", () => {
    open();
    const boxes = screen.getAllByRole("checkbox");
    expect(boxes).toHaveLength(3);
    expect(boxes[0]).toBeChecked();
    expect(boxes[0]).toBeDisabled();
    expect(boxes[1]).not.toBeChecked();
    expect(boxes[2]).not.toBeChecked();
  });

  it("promises to keep the settings while nothing destructive is selected", () => {
    open();
    expect(
      screen.getByText(en.tools.uninstall.keepSettings),
    ).toBeInTheDocument();
  });

  it("removes the app straight away when nothing else is selected", async () => {
    const onConfirm = open();
    await userEvent.click(
      screen.getByRole("button", { name: en.tools.uninstall.submit }),
    );
    expect(onConfirm).toHaveBeenCalledWith({
      removeSettings: false,
      removeCache: false,
    });
  });

  it("does not enable uninstall when native returns no app target", () => {
    PREVIEW.app = [];
    open();

    expect(
      screen.getByRole("button", { name: en.tools.uninstall.submit }),
    ).toBeDisabled();
  });

  it("states that removing settings deletes saved conversations too", async () => {
    open();
    await userEvent.click(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    );
    expect(
      screen.getByText(en.tools.uninstall.warning.sessionsInsideSettings),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(en.tools.uninstall.keepSettings),
    ).not.toBeInTheDocument();
  });

  it("does not claim conversations are at risk when the tool keeps them elsewhere", async () => {
    render(
      <UninstallModal
        tool={TOOL_WITHOUT_NESTED_SESSIONS}
        onOpenChange={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );
    await userEvent.click(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    );
    expect(
      screen.queryByText(en.tools.uninstall.warning.sessionsInsideSettings),
    ).toBeNull();
    // The outside-$HOME warning should still be there: it's unrelated to chat history and holds regardless of what's being deleted.
    expect(
      screen.getByText(en.tools.uninstall.warning.outsideHome),
    ).toBeInTheDocument();
  });

  it("keeps the saved-conversations warning visible on the confirmation screen", async () => {
    open();
    await userEvent.click(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.tools.uninstall.submit }),
    );
    expect(
      screen.getByText(en.tools.uninstall.confirmDescription),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.tools.uninstall.warning.sessionsInsideSettings),
    ).toBeInTheDocument();
  });

  it("requires a second confirmation before deleting user data", async () => {
    const onConfirm = open();
    await userEvent.click(
      screen.getByRole("checkbox", { name: en.tools.uninstall.option.cache }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.tools.uninstall.submit }),
    );
    expect(onConfirm).not.toHaveBeenCalled();
    expect(
      screen.getByText(en.tools.uninstall.confirmDescription),
    ).toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("button", { name: en.tools.uninstall.confirm }),
    );
    expect(onConfirm).toHaveBeenCalledWith({
      removeSettings: false,
      removeCache: true,
    });
  });

  it("can step back from the confirmation without losing the choices", async () => {
    open();
    await userEvent.click(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.tools.uninstall.submit }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.tools.uninstall.back }),
    );
    expect(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    ).toBeChecked();
  });

  it("resets the destructive checkboxes when reopened for the same tool", async () => {
    const { rerender } = render(
      <UninstallModal tool={TOOL} onOpenChange={vi.fn()} onConfirm={vi.fn()} />,
    );
    await userEvent.click(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    );
    expect(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    ).toBeChecked();

    // Close: tool goes back to null.
    rerender(
      <UninstallModal tool={null} onOpenChange={vi.fn()} onConfirm={vi.fn()} />,
    );

    // Reopen: the same tool.id.
    rerender(
      <UninstallModal tool={TOOL} onOpenChange={vi.fn()} onConfirm={vi.fn()} />,
    );

    expect(
      screen.getByRole("checkbox", {
        name: en.tools.uninstall.option.settings,
      }),
    ).not.toBeChecked();
    expect(
      screen.getByText(en.tools.uninstall.keepSettings),
    ).toBeInTheDocument();
  });

  it("renders nothing without a tool", () => {
    render(
      <UninstallModal tool={null} onOpenChange={vi.fn()} onConfirm={vi.fn()} />,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("cannot be dismissed by Escape while the removal is running", async () => {
    const onOpenChange = vi.fn();
    render(
      <UninstallModal
        tool={TOOL}
        busy
        onOpenChange={onOpenChange}
        onConfirm={vi.fn()}
      />,
    );
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
