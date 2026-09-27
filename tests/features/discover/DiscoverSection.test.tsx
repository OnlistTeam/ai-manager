import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { operationKeys } from "@/entities/operation";
import { DiscoverSection } from "@/features/discover";
import type { ExtensionScopeOption } from "@/features/extension-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

vi.mock("sonner", () => ({ toast: { error: vi.fn(), info: vi.fn() } }));

const CLAUDE: ExtensionScopeOption = {
  key: "tool:claude-code",
  scope: { kind: "tool", id: "claude-code" },
  name: "Claude Code",
  supported: true,
  tool: null,
  desktopApp: null,
};
const CODEX: ExtensionScopeOption = {
  key: "tool:codex",
  scope: { kind: "tool", id: "codex" },
  name: "Codex",
  supported: true,
  tool: null,
  desktopApp: null,
};
const DESKTOP: ExtensionScopeOption = {
  key: "desktop-app:claude-desktop",
  scope: { kind: "desktopApp", id: "claude-desktop" },
  name: "Claude Desktop",
  supported: true,
  tool: null,
  desktopApp: null,
};

function server_(overrides: Record<string, unknown>) {
  return {
    id: "deepwiki",
    name: "deepwiki",
    title: "DeepWiki",
    description: null,
    publisher: "Cognition",
    icon: null,
    homepage: true,
    transport: "http",
    runs: null,
    signIn: false,
    featured: true,
    inputs: [],
    added: null,
    ...overrides,
  };
}

const BRAVE = server_({
  id: "brave-search",
  name: "brave-search",
  title: "Brave Search",
  publisher: "Brave",
  transport: "stdio",
  runs: "npx",
  inputs: [
    {
      key: "BRAVE_API_KEY",
      target: "env",
      label: "BRAVE_API_KEY",
      kind: "apiKey",
      description: null,
      site: "brave.com/search/api",
      placeholder: null,
      secret: true,
      required: true,
    },
  ],
});

const REACH = [
  { scope: CLAUDE.scope, transports: ["stdio", "http", "sse"] },
  { scope: CODEX.scope, transports: ["stdio", "http"] },
  { scope: DESKTOP.scope, transports: ["stdio"] },
];

type Body = Record<string, unknown>;

function mount(
  kind: "skill" | "mcp",
  targets: ExtensionScopeOption[],
  blocked = false,
) {
  const client = createTestQueryClient();
  render(<DiscoverSection kind={kind} targets={targets} blocked={blocked} />, {
    wrapper: withQueryClient(client),
  });
  return client;
}

describe("DiscoverSection", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      {
        ds: en.ds,
        error: en.error,
        extensions: en.extensions,
        discover: en.discover,
      },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows placeholders first, then featured servers in the reader's words", async () => {
    let answer: (() => void) | null = null;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_mcp_list`, async () => {
        await new Promise<void>((resolve) => {
          answer = resolve;
        });
        return HttpResponse.json({
          items: [server_({}), BRAVE],
          reach: REACH,
          sourceError: null,
        });
      }),
    );
    mount("mcp", [CLAUDE]);

    expect(
      await screen.findByLabelText("Loading suggestions…"),
    ).toHaveAttribute("aria-busy", "true");
    await waitFor(() => expect(answer).not.toBeNull());
    act(() => answer?.());

    const list = await screen.findByRole("list", { name: "Discover" });
    expect(within(list).getAllByRole("listitem")).toHaveLength(2);
    expect(
      screen.getByText(en.discover.mcp.featured.deepwiki),
    ).toBeInTheDocument();
    expect(screen.getByText(en.discover.source.mcp)).toBeInTheDocument();
    expect(screen.getByLabelText("Needs API key")).toBeInTheDocument();
    expect(
      screen.getByRole("searchbox", { name: "Search MCP servers" }),
    ).toBeInTheDocument();
  });

  it("adds a server that needs nothing straight from its card, to every app that can reach it", async () => {
    const installs: Body[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_mcp_list`, () =>
        HttpResponse.json({
          items: [server_({})],
          reach: REACH,
          sourceError: null,
        }),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_discover_mcp_install`,
        async ({ request }) => {
          installs.push((await request.json()) as Body);
          return HttpResponse.json("op-1");
        },
      ),
    );
    const client = mount("mcp", [CLAUDE, CODEX, DESKTOP]);

    await userEvent.click(
      await screen.findByRole("button", { name: "Add DeepWiki" }),
    );

    await waitFor(() => expect(installs).toHaveLength(1));
    expect(installs[0]).toMatchObject({
      server: "deepwiki",
      values: [],
      description: en.discover.mcp.featured.deepwiki,
      scopes: [CLAUDE.scope, CODEX.scope],
    });
    expect(
      await screen.findByRole("button", { name: "Add DeepWiki" }),
    ).toHaveAttribute("aria-busy", "true");

    act(() => {
      client.setQueryData(operationKeys.list(), [
        { id: "op-1", status: "success" },
      ]);
    });
    expect(await screen.findByText("Added")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Add DeepWiki" }),
    ).not.toBeInTheDocument();
  });

  it("asks for a required key in the details dialog before adding", async () => {
    const installs: Body[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_mcp_list`, () =>
        HttpResponse.json({ items: [BRAVE], reach: REACH, sourceError: null }),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_discover_mcp_install`,
        async ({ request }) => {
          installs.push((await request.json()) as Body);
          return HttpResponse.json("op-2");
        },
      ),
    );
    mount("mcp", [CLAUDE, DESKTOP]);

    await userEvent.click(
      await screen.findByRole("button", { name: "Add Brave Search" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "Brave Search" });
    // The hint is shown once and read once (Field keeps an sr-only copy).
    expect(
      within(dialog).getAllByText(
        "Get one at brave.com/search/api · Saved as BRAVE_API_KEY",
      ),
    ).not.toHaveLength(0);
    const key = within(dialog).getByLabelText("API key");
    expect(key).toHaveAttribute("type", "password");

    await userEvent.click(within(dialog).getByRole("button", { name: "Add" }));
    expect(
      await within(dialog).findByText("Fill in API key to continue."),
    ).toBeInTheDocument();
    expect(installs).toHaveLength(0);

    await userEvent.type(key, "secret-value");
    await userEvent.click(
      within(dialog).getByRole("button", {
        name: "Claude Desktop: Brave Search",
      }),
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Add" }));

    await waitFor(() => expect(installs).toHaveLength(1));
    expect(installs[0]).toMatchObject({
      server: "brave-search",
      values: [{ key: "BRAVE_API_KEY", value: "secret-value" }],
      scopes: [CLAUDE.scope],
    });
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });

  it("names apps that cannot run a remote server and leaves them out", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_mcp_list`, () =>
        HttpResponse.json({
          items: [server_({})],
          reach: REACH,
          sourceError: null,
        }),
      ),
    );
    mount("mcp", [CLAUDE, DESKTOP]);

    await userEvent.click(
      await screen.findByRole("button", { name: "Details for DeepWiki" }),
    );
    const dialog = await screen.findByRole("dialog", { name: "DeepWiki" });
    expect(
      within(dialog).getByText("Can't run this server: Claude Desktop"),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByRole("button", { name: "Claude Code: DeepWiki" }),
    ).toHaveAttribute("aria-pressed", "true");
  });

  it("marks what is already here instead of offering Add", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_mcp_list`, () =>
        HttpResponse.json({
          items: [server_({ added: "my-wiki" })],
          reach: REACH,
          sourceError: null,
        }),
      ),
    );
    mount("mcp", [CLAUDE]);

    expect(
      await screen.findByLabelText("Already here as my-wiki"),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Add DeepWiki" }),
    ).not.toBeInTheDocument();
  });

  it("keeps featured cards and says so quietly when the registry is down", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_mcp_list`, () =>
        HttpResponse.json({
          items: [server_({})],
          reach: REACH,
          sourceError: {
            code: "NETWORK_ERROR",
            messageKey: "error.discover.registryUnreachable",
            technicalMessage: "registry: timed out",
            remediation: null,
            contextId: null,
          },
        }),
      ),
    );
    mount("mcp", [CLAUDE]);

    expect(
      await screen.findByText(en.error.discover.registryUnreachable),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add DeepWiki" })).toBeEnabled();
  });

  it("searches after a pause, with the words typed", async () => {
    const queries: string[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_discover_mcp_list`,
        async ({ request }) => {
          const { query } = (await request.json()) as { query: string };
          queries.push(query);
          return HttpResponse.json({
            items: query ? [BRAVE] : [server_({})],
            reach: REACH,
            sourceError: null,
          });
        },
      ),
    );
    mount("mcp", [CLAUDE]);
    await screen.findByRole("button", { name: "Add DeepWiki" });

    await userEvent.type(
      screen.getByRole("searchbox", { name: "Search MCP servers" }),
      "brave",
    );
    expect(await screen.findByText("1 result")).toBeInTheDocument();
    expect(queries).toEqual(["", "brave"]);
  });

  it("describes Skills lazily and adds one for every app", async () => {
    const installs: Body[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_skill_list`, () =>
        HttpResponse.json({
          items: [
            {
              id: "anthropics/skills/pdf",
              source: "anthropics/skills",
              skillId: "pdf",
              name: "pdf",
              installs: 12_345,
              official: true,
              icon: "https://github.com/anthropics.png?size=96",
              description: null,
              added: null,
            },
          ],
          sourceError: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_discover_skill_descriptions`, () =>
        HttpResponse.json({
          "anthropics/skills/pdf": "Fill in and sign PDF forms.",
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_discover_icon`, () =>
        HttpResponse.json(
          {
            code: "NETWORK_ERROR",
            messageKey: "error.discover.iconUnavailable",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_discover_skill_install`,
        async ({ request }) => {
          installs.push((await request.json()) as Body);
          return HttpResponse.json("op-3");
        },
      ),
    );
    mount("skill", [CLAUDE, CODEX]);

    expect(
      await screen.findByText("Fill in and sign PDF forms."),
    ).toBeInTheDocument();
    expect(screen.getByText(en.discover.source.skill)).toBeInTheDocument();
    expect(screen.getByLabelText(en.discover.officialHint)).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Add pdf" }));
    await waitFor(() => expect(installs).toHaveLength(1));
    expect(installs[0]).toEqual({
      skill: "anthropics/skills/pdf",
      tools: ["claude-code", "codex"],
    });
  });

  it("shows a refused add once, in the section", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_discover_skill_list`, () =>
        HttpResponse.json({
          items: [
            {
              id: "a/b/c",
              source: "a/b",
              skillId: "c",
              name: "c",
              installs: 1,
              official: false,
              icon: "https://github.com/a.png?size=96",
              description: "Does c.",
              added: null,
            },
          ],
          sourceError: null,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_discover_skill_install`, () =>
        HttpResponse.json(
          {
            code: "EXTENSION_NOT_FOUND",
            messageKey: "error.discover.skillMissing",
            technicalMessage: "a/b has 0 Skills",
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount("skill", [CLAUDE]);

    await userEvent.click(await screen.findByRole("button", { name: "Add c" }));
    expect(
      await screen.findByText(
        `c wasn't added. ${en.error.discover.skillMissing}`,
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Add c" })).toBeEnabled();
  });
});
