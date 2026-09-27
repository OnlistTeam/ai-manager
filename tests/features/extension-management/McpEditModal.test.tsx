import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  McpInstallModal,
  type McpEditTarget,
} from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({ info: vi.fn(), success: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

const SAVED = {
  id: "files-a1b2c3d4",
  name: "Files",
  description: "Reads the notes folder.",
  connection: {
    transport: "stdio",
    command: "npx",
    arguments: ["-y", "server-files"],
    env: [{ name: "FILES_TOKEN", value: "tok-1" }],
  },
};

function mount(editing: McpEditTarget, onOpenChange = vi.fn()) {
  return {
    onOpenChange,
    ...render(
      <McpInstallModal
        open
        scope={{ kind: "tool", id: "codex" }}
        scopeName="Codex"
        mutationsBlocked={false}
        editing={editing}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    ),
  };
}

const managed: McpEditTarget = {
  id: "files-a1b2c3d4",
  name: "Files",
  scope: { kind: "tool", id: "claude-code" },
  found: [],
};

describe("McpInstallModal in edit mode", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, extensions: en.extensions },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.success.mockClear();
  });

  it("prefills the saved connection, values included, and saves it in place", async () => {
    let getBody: unknown;
    let updateBody: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_get`, async ({ request }) => {
        getBody = await request.json();
        return HttpResponse.json(SAVED);
      }),
      http.post(`${TAURI_ENDPOINT}/app_mcp_update`, async ({ request }) => {
        updateBody = await request.json();
        return HttpResponse.json([]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_extensions_adopt_detected`, () => {
        throw new Error("a managed connection is not taken over again");
      }),
    );
    const { onOpenChange } = mount(managed);

    const dialog = screen.getByRole("dialog", {
      name: en.extensions.mcp.edit.title.replace("{{name}}", "Files"),
    });
    const name = within(dialog).getByLabelText(en.extensions.mcp.install.name);
    await waitFor(() => expect(name).toHaveValue("Files"));
    expect(getBody).toEqual({
      scope: { kind: "tool", id: "claude-code" },
      mcp: "files-a1b2c3d4",
    });
    expect(
      within(dialog).getByLabelText(en.extensions.mcp.install.arguments),
    ).toHaveValue("-y\nserver-files");
    expect(
      within(dialog).getByLabelText(en.extensions.mcp.install.variable.value),
    ).toHaveValue("tok-1");
    // Paste is for starting a connection, not for editing one.
    expect(
      within(dialog).queryByRole("button", {
        name: en.extensions.mcp.install.paste.open,
      }),
    ).toBeNull();

    const args = within(dialog).getByLabelText(
      en.extensions.mcp.install.arguments,
    );
    await userEvent.clear(args);
    await userEvent.type(args, "-y{enter}server-files@2");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.edit.confirm,
      }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(updateBody).toEqual({
      scope: { kind: "tool", id: "claude-code" },
      mcp: "files-a1b2c3d4",
      draft: {
        name: "Files",
        description: "Reads the notes folder.",
        connection: {
          transport: "stdio",
          command: "npx",
          arguments: ["-y", "server-files@2"],
          env: [{ name: "FILES_TOKEN", value: "tok-1" }],
        },
      },
    });
    expect(toastMocks.success).toHaveBeenCalled();
  });

  it("takes a found connection over from every app it is in before saving", async () => {
    const calls: string[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_get`, () =>
        HttpResponse.json({ ...SAVED, id: "browser", name: "browser" }),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_extensions_adopt_detected`,
        async ({ request }) => {
          const body = (await request.json()) as {
            scope: { id: string };
            enabled: boolean;
          };
          calls.push(`adopt:${body.scope.id}:${body.enabled}`);
          return HttpResponse.json([]);
        },
      ),
      http.post(`${TAURI_ENDPOINT}/app_mcp_update`, () => {
        calls.push("update");
        return HttpResponse.json([]);
      }),
    );
    const { onOpenChange } = mount({
      id: "browser",
      name: "browser",
      scope: { kind: "tool", id: "claude-code" },
      found: [
        { kind: "tool", id: "claude-code" },
        { kind: "tool", id: "codex" },
      ],
    });

    const dialog = screen.getByRole("dialog");
    expect(dialog).toHaveTextContent(en.extensions.mcp.edit.descriptionFound);
    await waitFor(() =>
      expect(
        within(dialog).getByLabelText(en.extensions.mcp.install.name),
      ).toHaveValue("browser"),
    );
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.edit.confirm,
      }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(calls).toEqual([
      "adopt:claude-code:true",
      "adopt:codex:true",
      "update",
    ]);
  });

  it("says so and blocks Save when the saved connection cannot be read", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_get`, () =>
        HttpResponse.json(
          {
            code: "CONFIG_PARSE_FAILED",
            messageKey: "error.mcp.editUnavailable",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount(managed);

    const dialog = screen.getByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      en.error.mcp.editUnavailable,
    );
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.edit.confirm,
      }),
    ).toBeDisabled();
  });
});
