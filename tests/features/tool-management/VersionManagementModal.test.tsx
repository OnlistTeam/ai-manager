import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { VersionManagementModal } from "@/features/tool-management";
import en from "@/i18n/locales/en.json";
import type { Tool } from "@/entities/tool";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const TOOL: Tool = {
  id: "codex",
  name: "Codex CLI",
  descriptionKey: "tool.codex.description",
  status: "installed",
  version: "1.0.0",
  latestVersion: "1.2.0",
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: true,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: false,
    canManageVersion: true,
    canLaunch: true,
  },
  sessionsInsideSettings: false,
  environment: "macos",
};

function mount(onConfirm = vi.fn(), tool: Tool = TOOL) {
  const client = createTestQueryClient();
  render(
    <VersionManagementModal
      tool={tool}
      onOpenChange={vi.fn()}
      onConfirm={onConfirm}
    />,
    { wrapper: withQueryClient(client) },
  );
  return onConfirm;
}

describe("VersionManagementModal", () => {
  beforeEach(async () => {
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: vi.fn(),
    });
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, tools: en.tools },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("selects the recommended latest version by default", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_version_catalog`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            tool: "codex",
            source: "npm",
            canChangeVersion: true,
            restriction: null,
            latestVersion: "1.2.0",
            versions: ["1.2.0", "1.1.0", "1.0.0"],
            mirrorUsed: true,
          });
        },
      ),
    );
    const onConfirm = mount();

    const select = await screen.findByLabelText(en.tools.version.target);
    expect(select).toHaveTextContent(
      en.tools.version.defaultOption.replace("{{version}}", "1.2.0"),
    );
    const confirm = screen.getByRole("button", {
      name: en.tools.version.confirm,
    });
    expect(confirm).toBeEnabled();
    expect(screen.getByText(en.tools.version.mirrorUsed)).toBeInTheDocument();

    await userEvent.click(confirm);

    expect(onConfirm).toHaveBeenCalledWith("1.2.0");
    expect(bodies).toEqual([{ tool: "codex" }]);
  });

  it("automatically recommends Grok's newest stable release", async () => {
    const onConfirm = vi.fn();
    const grok: Tool = {
      ...TOOL,
      id: "grok-build",
      name: "Grok Build",
      version: "0.1.4",
      latestVersion: "1.0.5",
      capabilities: { ...TOOL.capabilities, canRepair: false },
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_version_catalog`, () =>
        HttpResponse.json({
          tool: "grok-build",
          source: "npm",
          canChangeVersion: true,
          restriction: null,
          latestVersion: "1.0.12",
          distTags: [
            { tag: "latest", version: "0.1.4" },
            { tag: "alpha", version: "1.1.0-alpha.1" },
          ],
          versions: [
            "1.1.0-alpha.1",
            "1.0.12",
            "1.0.11",
            "1.0.5",
            "0.1.4",
            "0.1.3",
          ],
          mirrorUsed: false,
        }),
      ),
    );
    mount(onConfirm, grok);

    const select = await screen.findByRole("combobox", {
      name: en.tools.version.target,
    });
    expect(select).toHaveTextContent(
      en.tools.version.defaultOption.replace("{{version}}", "1.0.12"),
    );
    expect(
      screen.getByText(
        en.tools.version.defaultChannelNote.replace("{{version}}", "1.0.12"),
      ),
    ).toBeInTheDocument();
    const confirm = screen.getByRole("button", {
      name: en.tools.version.confirm,
    });
    expect(confirm).toBeEnabled();

    await userEvent.click(select);
    expect(
      await screen.findByText(en.tools.version.taggedGroup),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("option", {
        name: en.tools.version.taggedOption
          .replace("{{version}}", "1.1.0-alpha.1")
          .replace("{{tags}}", "alpha"),
      }),
    ).toBeInTheDocument();
    expect(
      await screen.findByText(en.tools.version.otherPublishedGroup),
    ).toBeInTheDocument();
    await userEvent.click(screen.getByRole("option", { name: "1.0.11" }));
    expect(
      screen.getByText(en.tools.version.nonDefaultWarning),
    ).toBeInTheDocument();
    expect(confirm).toBeEnabled();

    await userEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledWith("1.0.11");
  });

  it("does not cross from an official installer into a second package-manager copy", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_version_catalog`, () =>
        HttpResponse.json({
          tool: "codex",
          source: "nativeInstaller",
          canChangeVersion: false,
          restriction: "nativeInstaller",
          latestVersion: null,
          versions: [],
          mirrorUsed: false,
        }),
      ),
    );
    mount();

    expect(
      await screen.findByText(en.tools.version.restrictedTitle),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.tools.version.restriction.nativeInstaller),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: en.tools.version.confirm }),
      ).toBeNull(),
    );
  });
});
