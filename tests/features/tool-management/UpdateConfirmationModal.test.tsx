import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import {
  UpdateConfirmationModal,
  type UpdateConfirmationModalProps,
} from "@/features/tool-management";
import type { Tool, ToolUpdatePreview } from "@/native";

const CLAUDE: Tool = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "tool.claude-code.description",
  status: "updateAvailable",
  version: "2.1.210",
  latestVersion: "2.1.211",
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: false,
    canLaunch: true,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: true,
    canManageVersion: true,
  },
  sessionsInsideSettings: false,
  environment: "macos",
};

const CODEX: Tool = { ...CLAUDE, id: "codex", name: "Codex" };

const NATIVE_READY: ToolUpdatePreview = {
  state: "ready",
  preview: {
    tool: "claude-code",
    previewFingerprint: "a".repeat(64),
    targetVersion: "2.1.211",
    source: "nativeInstaller",
    installations: [
      {
        source: "nativeInstaller",
        version: "2.1.210",
        runnable: true,
        isDefault: true,
        location: "/Users/test/.local/bin/claude",
      },
    ],
    attempts: [
      {
        method: "nativeSelfUpdate",
        commands: ["/Users/test/.local/bin/claude update"],
      },
      {
        method: "officialInstaller",
        commands: ["bash -c [official installer]"],
      },
    ],
    multipleInstallations: false,
  },
};

const BLOCKED: ToolUpdatePreview = {
  state: "blocked",
  tool: "codex",
  reason: "ambiguousInstallation",
};

function renderModal(overrides: Partial<UpdateConfirmationModalProps> = {}) {
  const props: UpdateConfirmationModalProps = {
    open: true,
    tools: [CLAUDE],
    previews: [NATIVE_READY],
    loading: false,
    refreshing: false,
    previewError: null,
    submitting: false,
    onOpenChange: vi.fn(),
    onRefresh: vi.fn(),
    onConfirm: vi.fn(),
    ...overrides,
  };
  render(<UpdateConfirmationModal {...props} />);
  return props;
}

describe("UpdateConfirmationModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, tools: en.tools, home: en.home },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("explains a native update in plain language and folds commands by default", async () => {
    renderModal();
    expect(screen.getByText("Uses Claude Code's own updater")).toBeVisible();
    expect(
      screen.getByText(
        "If that cannot finish, AI Manager retries with the vendor's official installer.",
      ),
    ).toBeVisible();
    const command = screen.getByText(/claude update/);
    expect(command).not.toBeVisible();

    await userEvent.click(screen.getByText("Technical details"));
    expect(command).toBeVisible();
  });

  it("lists every detected installation and marks the selected target", () => {
    const multi: ToolUpdatePreview = {
      state: "ready",
      preview: {
        ...NATIVE_READY.preview,
        installations: [
          ...NATIVE_READY.preview.installations,
          {
            source: "brew",
            version: "2.1.209",
            runnable: true,
            isDefault: false,
            location: "/opt/homebrew/bin/claude",
          },
        ],
        multipleInstallations: true,
      },
    };
    renderModal({ previews: [multi] });

    expect(screen.getByText("2 installations detected")).toBeVisible();
    expect(
      screen.getByText(/^Current command-line default · Version/),
    ).toBeVisible();
    expect(screen.getByText("/Users/test/.local/bin/claude")).not.toBeVisible();
  });

  it("blocks a single unavailable preview and offers a recheck", async () => {
    const onRefresh = vi.fn();
    renderModal({ tools: [CODEX], previews: [BLOCKED], onRefresh });

    expect(screen.getByText("Codex will be skipped")).toBeVisible();
    expect(screen.getByRole("button", { name: "Update" })).toBeDisabled();
    await userEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(onRefresh).toHaveBeenCalledOnce();
  });

  it("keeps ready items actionable while blocked bulk items stay visible", () => {
    renderModal({ tools: [CLAUDE, CODEX], previews: [NATIVE_READY, BLOCKED] });

    expect(screen.getByText("Claude Code will update")).toBeVisible();
    expect(screen.getByText("Codex will be skipped")).toBeVisible();
    expect(screen.getByRole("button", { name: "Update 1 tool" })).toBeEnabled();
  });

  it("uses safe loading and query-error states without showing a guessed method", () => {
    renderModal({ previews: undefined, loading: true });
    expect(
      screen.getByRole("status", { name: "Checking update details" }),
    ).toBeVisible();
    expect(screen.getByRole("button", { name: "Update" })).toBeDisabled();
    expect(screen.queryByText(/own updater|Homebrew/i)).toBeNull();

    cleanup();
    renderModal({
      previews: undefined,
      previewError: new Error("private /Users/test/path"),
    });
    expect(
      screen.getByRole("alert", { name: "Could not check update details" }),
    ).toBeVisible();
    expect(document.body).not.toHaveTextContent("private /Users/test/path");
    expect(screen.getByRole("button", { name: "Update" })).toBeDisabled();
  });

  it("keeps initial focus inside the dialog while actions are unavailable", () => {
    renderModal({ previews: undefined, loading: true });
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();

    cleanup();
    renderModal({ actionPaused: true });
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("moves focus to recheck when loading resolves to blocked", () => {
    const { rerender } = render(
      <UpdateConfirmationModal
        open
        tools={[CODEX]}
        previews={undefined}
        loading
        refreshing={false}
        previewError={null}
        submitting={false}
        onOpenChange={vi.fn()}
        onRefresh={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();

    rerender(
      <UpdateConfirmationModal
        open
        tools={[CODEX]}
        previews={[BLOCKED]}
        loading={false}
        refreshing={false}
        previewError={null}
        submitting={false}
        onOpenChange={vi.fn()}
        onRefresh={vi.fn()}
        onConfirm={vi.fn()}
      />,
    );
    expect(screen.getByRole("button", { name: "Check again" })).toHaveFocus();
  });

  it("moves focus from a completed refresh to the ready update", () => {
    const base: UpdateConfirmationModalProps = {
      open: true,
      tools: [CLAUDE],
      previews: [NATIVE_READY],
      loading: false,
      refreshing: true,
      previewError: null,
      submitting: false,
      onOpenChange: vi.fn(),
      onRefresh: vi.fn(),
      onConfirm: vi.fn(),
    };
    const { rerender } = render(<UpdateConfirmationModal {...base} />);
    const recheck = screen.getByRole("button", { name: "Check again" });
    recheck.focus();

    rerender(<UpdateConfirmationModal {...base} refreshing={false} />);
    expect(screen.getByRole("button", { name: "Update" })).toHaveFocus();
  });

  it("disables cancel during the hand-off but still lets the user leave", async () => {
    const onOpenChange = vi.fn();
    renderModal({ tools: [CODEX], previews: [BLOCKED], onOpenChange });
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(onOpenChange).toHaveBeenCalledWith(false);

    cleanup();
    onOpenChange.mockClear();
    renderModal({ submitting: true, onOpenChange });
    const dialog = screen.getByRole("dialog");

    // Cancel can no longer undo anything once the hand-off is in flight.
    expect(
      within(dialog).getByRole("button", { name: "Cancel" }),
    ).toBeDisabled();

    // Leaving is a different thing from cancelling, and it stays available:
    // the task is on its way to the activity panel either way.
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Close" }),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);

    onOpenChange.mockClear();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });
});
