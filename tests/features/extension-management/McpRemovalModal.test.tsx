import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Extension } from "@/entities/extension";
import { McpRemovalModal } from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const connection: Extension = {
  kind: "mcp",
  id: "filesystem-a1b2c3d4",
  scope: { kind: "tool", id: "codex" },
  name: "Project files",
  description: "Read selected project files.",
  management: "managed",
  enabled: true,
  canDisable: true,
};

const toastMocks = vi.hoisted(() => ({ info: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

function mount(onOpenChange = vi.fn(), mutationsBlocked = false) {
  return {
    onOpenChange,
    ...render(
      <McpRemovalModal
        connection={connection}
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    ),
  };
}

describe("McpRemovalModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.info.mockClear();
  });

  it("defaults to Cancel and explains the exact global impact", () => {
    mount();
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.mcp.remove.title.replace("{{name}}", connection.name),
    });
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toHaveFocus();
    expect(dialog).toHaveTextContent(en.extensions.mcp.remove.point.everywhere);
    expect(dialog).toHaveTextContent(
      en.extensions.mcp.remove.point.keepsTargets,
    );
    expect(dialog).toHaveTextContent(en.extensions.mcp.remove.point.noUndo);
  });

  it("keeps Cancel available but blocks removal when tool authority is stale", () => {
    mount(vi.fn(), true);
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.mcp.remove.title.replace("{{name}}", connection.name),
    });
    expect(
      within(dialog).getByRole("alert", {
        name: en.extensions.actionsPaused.title,
      }),
    ).toHaveTextContent(en.extensions.actionsPaused.description);
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.remove.confirm,
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
  });

  it("queues only the authoritative connection id and closes", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_remove`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-mcp-remove-1");
      }),
    );
    const { onOpenChange } = mount();
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.remove.confirm }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({
      scope: { kind: "tool", id: "codex" },
      mcp: "filesystem-a1b2c3d4",
    });
    expect(toastMocks.info).toHaveBeenCalledWith("Removing Project files", {
      description: en.extensions.mcp.remove.queuedDescription,
    });
  });

  it("keeps safe native failures inline without technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_remove`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.mcp.removeRestoreFailed",
            technicalMessage: "/private/config token=do-not-show",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.remove.confirm }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.error.mcp.removeRestoreFailed);
    expect(alert).toHaveTextContent(en.extensions.mcp.remove.errorRetry);
    expect(alert).not.toHaveTextContent("/private/config");
    expect(alert).not.toHaveTextContent("do-not-show");
  });

  it("blocks Cancel, close, and Escape while the start request is pending", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_remove`, async () => {
        await delay("infinite");
        return HttpResponse.json("never");
      }),
    );
    const { onOpenChange } = mount();
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.remove.confirm }),
    );

    expect(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    ).toBeDisabled();
    expect(
      screen.queryByRole("button", { name: en.ds.action.close }),
    ).toBeNull();
    await userEvent.keyboard("{Escape}");
    expect(onOpenChange).not.toHaveBeenCalled();
  });
});
