import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { delay, http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { McpInstallModal } from "@/features/extension-management";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({ info: vi.fn() }));
vi.mock("sonner", () => ({ toast: toastMocks }));

function mount(onOpenChange = vi.fn(), mutationsBlocked = false) {
  return {
    onOpenChange,
    ...render(
      <McpInstallModal
        open
        scope={{ kind: "tool", id: "claude-code" }}
        scopeName="Claude Code"
        mutationsBlocked={mutationsBlocked}
        onOpenChange={onOpenChange}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    ),
  };
}

async function fillLocalConnection() {
  await userEvent.type(
    screen.getByLabelText(en.extensions.mcp.install.name),
    "Filesystem",
  );
  await userEvent.type(
    screen.getByLabelText(en.extensions.mcp.install.command),
    "npx",
  );
  await userEvent.type(
    screen.getByLabelText(en.extensions.mcp.install.arguments),
    "-y{enter}@modelcontextprotocol/server-filesystem{enter}/Users/me/Notes",
  );
}

describe("McpInstallModal", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
        taskCenter: en.taskCenter,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
    toastMocks.info.mockClear();
  });

  it("starts on the friendly local form and focuses the connection name", () => {
    const { container } = mount();
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.mcp.install.title,
    });
    expect(
      within(dialog).getByLabelText(en.extensions.mcp.install.name),
    ).toHaveFocus();
    expect(
      within(dialog).getByRole("radio", {
        name: /Local command/,
      }),
    ).toBeChecked();
    expect(container.querySelector("pre, code")).toBeNull();
    expect(
      within(dialog).queryByLabelText(en.extensions.mcp.install.paste.label),
    ).toBeNull();
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.install.paste.open,
      }),
    ).toBeEnabled();
    expect(
      within(dialog).getByRole("group", {
        name: en.extensions.mcp.install.env.label,
      }),
    ).toBeInTheDocument();
    expect(
      within(dialog).queryAllByLabelText(
        en.extensions.mcp.install.variable.value,
      ),
    ).toHaveLength(0);
    expect(dialog.querySelector("#mcp-install-form")).toHaveClass(
      "scrollbar-subtle",
    );
  });

  it("keeps the draft and Cancel available but blocks submission when tool authority is stale", () => {
    mount(vi.fn(), true);
    const dialog = screen.getByRole("dialog", {
      name: en.extensions.mcp.install.title,
    });
    expect(
      within(dialog).getByRole("alert", {
        name: en.extensions.actionsPaused.title,
      }),
    ).toHaveTextContent(en.extensions.actionsPaused.description);
    expect(
      within(dialog).getByLabelText(en.extensions.mcp.install.name),
    ).toBeEnabled();
    expect(
      within(dialog).getByRole("button", {
        name: en.extensions.mcp.install.confirm,
      }),
    ).toBeDisabled();
    expect(
      within(dialog).getByRole("button", { name: en.ds.action.cancel }),
    ).toBeEnabled();
  });

  it("queues an exact typed local draft without ids, JSON, or secret fields", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-mcp-1");
      }),
    );
    const { onOpenChange } = mount();
    await fillLocalConnection();
    await userEvent.type(
      screen.getByLabelText(en.extensions.mcp.install.optionalDescription),
      "Read selected notes",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.install.confirm }),
    );

    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({
      scope: { kind: "tool", id: "claude-code" },
      draft: {
        name: "Filesystem",
        description: "Read selected notes",
        connection: {
          transport: "stdio",
          command: "npx",
          arguments: [
            "-y",
            "@modelcontextprotocol/server-filesystem",
            "/Users/me/Notes",
          ],
          env: [],
        },
      },
    });
    expect(body).not.toHaveProperty("id");
    expect(JSON.stringify(body)).not.toMatch(/"(headers|token|mcpServers)"/);
    expect(toastMocks.info).toHaveBeenCalledWith("Adding Filesystem", {
      description: "You can keep working and follow progress in Activity.",
    });
  });

  it("rejects public HTTP inline, then sends only a secure remote endpoint", async () => {
    let requests = 0;
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, async ({ request }) => {
        requests += 1;
        body = await request.json();
        return HttpResponse.json("op-mcp-remote");
      }),
    );
    mount();
    await userEvent.type(
      screen.getByLabelText(en.extensions.mcp.install.name),
      "Docs service",
    );
    await userEvent.click(screen.getByRole("radio", { name: /HTTP/ }));
    const url = screen.getByLabelText(en.extensions.mcp.install.url);
    await userEvent.type(url, "http://mcp.example.com/api");
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.install.confirm }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.mcp.urlInsecure,
    );
    expect(requests).toBe(0);

    await userEvent.clear(url);
    await userEvent.type(url, "https://mcp.example.com/api");
    expect(screen.getByRole("radio", { name: /HTTP/ })).toBeChecked();
    expect(url).toHaveValue("https://mcp.example.com/api");
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.install.confirm }),
    );
    await waitFor(() => expect(requests).toBe(1));
    expect(body).toEqual({
      scope: { kind: "tool", id: "claude-code" },
      draft: {
        name: "Docs service",
        description: null,
        connection: {
          transport: "http",
          url: "https://mcp.example.com/api",
          headers: [],
        },
      },
    });
  });

  it("fills the form from pasted README JSON and sends its environment variables", async () => {
    let body: unknown;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, async ({ request }) => {
        body = await request.json();
        return HttpResponse.json("op-mcp-paste");
      }),
    );
    const { onOpenChange } = mount();
    const copy = en.extensions.mcp.install;
    await userEvent.click(
      screen.getByRole("button", { name: copy.paste.open }),
    );
    const pasteBox = screen.getByLabelText(copy.paste.label);
    expect(pasteBox).toHaveFocus();

    await userEvent.type(pasteBox, "not json");
    await userEvent.click(
      screen.getByRole("button", { name: copy.paste.apply }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent(copy.paste.invalid);

    await userEvent.clear(pasteBox);
    await userEvent.click(pasteBox);
    await userEvent.paste(
      JSON.stringify({
        mcpServers: {
          github: {
            command: "npx",
            args: ["-y", "@modelcontextprotocol/server-github"],
            env: { GITHUB_TOKEN: "ghp_example" },
          },
          extra: { command: "uvx" },
        },
      }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: copy.paste.apply }),
    );

    expect(screen.queryByLabelText(copy.paste.label)).toBeNull();
    expect(screen.getByRole("status")).toHaveTextContent(
      copy.paste.filledFirst.replace("{{name}}", "github"),
    );
    expect(screen.getByLabelText(copy.name)).toHaveValue("github");
    expect(screen.getByLabelText(copy.command)).toHaveValue("npx");
    expect(screen.getByLabelText(copy.variable.name)).toHaveValue(
      "GITHUB_TOKEN",
    );
    expect(screen.getByLabelText(copy.variable.value)).toHaveValue(
      "ghp_example",
    );

    await userEvent.click(screen.getByRole("button", { name: copy.confirm }));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(body).toEqual({
      scope: { kind: "tool", id: "claude-code" },
      draft: {
        name: "github",
        description: null,
        connection: {
          transport: "stdio",
          command: "npx",
          arguments: ["-y", "@modelcontextprotocol/server-github"],
          env: [{ name: "GITHUB_TOKEN", value: "ghp_example" }],
        },
      },
    });
  });

  it("adds and removes header rows and checks them before sending", async () => {
    let requests = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, () => {
        requests += 1;
        return HttpResponse.json("op-mcp-headers");
      }),
    );
    mount();
    const copy = en.extensions.mcp.install;
    await userEvent.type(screen.getByLabelText(copy.name), "Docs");
    await userEvent.click(screen.getByRole("radio", { name: /HTTP/ }));
    await userEvent.type(
      screen.getByLabelText(copy.url),
      "https://mcp.example.com/api",
    );
    await userEvent.click(
      screen.getByRole("button", { name: copy.headers.add }),
    );
    expect(screen.getByLabelText(copy.variable.name)).toHaveFocus();
    await userEvent.keyboard("X Api Key");
    await userEvent.type(screen.getByLabelText(copy.variable.value), "abc");
    await userEvent.click(screen.getByRole("button", { name: copy.confirm }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      en.error.mcp.headersInvalid,
    );
    expect(requests).toBe(0);

    await userEvent.click(
      screen.getByRole("button", { name: copy.variable.remove }),
    );
    expect(screen.queryByLabelText(copy.variable.name)).toBeNull();
    expect(
      screen.getByRole("button", { name: copy.headers.add }),
    ).toHaveFocus();
  });

  it("keeps safe native failures inline and never reveals technical details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "INSTALL_FAILED",
            messageKey: "error.mcp.installFailed",
            technicalMessage: "/private/config token=do-not-show",
            remediation: "error.remediation.checkMcpSettings",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    mount();
    await fillLocalConnection();
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.install.confirm }),
    );
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.error.mcp.installFailed);
    expect(alert).toHaveTextContent(en.error.remediation.checkMcpSettings);
    expect(alert).not.toHaveTextContent("/private/config");
    expect(alert).not.toHaveTextContent("do-not-show");
  });

  it("blocks Cancel, close, and Escape while the start request is pending", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, async () => {
        await delay("infinite");
        return HttpResponse.json("never");
      }),
    );
    const { onOpenChange } = mount();
    await fillLocalConnection();
    await userEvent.click(
      screen.getByRole("button", { name: en.extensions.mcp.install.confirm }),
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
