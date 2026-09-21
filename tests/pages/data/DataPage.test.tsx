import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { toolKeys, type Tool } from "@/entities/tool";
import en from "@/i18n/locales/en.json";
import { DataPage } from "@/pages/data/DataPage";
import { ToolDetailsModal } from "@/pages/tools/ToolDetailsModal";
import { DataOverviewStrip } from "@/pages/data/DataOverviewStrip";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const endpoint = "http://tauri.local";
const capabilities = {
  canInstall: true,
  canUpdate: true,
  canUninstall: true,
  canRepair: false,
  canManageProvider: true,
  canManageMcp: true,
  canManageSkills: true,
  canManagePrompts: true,
  canManageVersion: true,
  canLaunch: true,
};
function tool(overrides: Partial<Tool> = {}): Tool {
  return {
    id: "claude-code",
    name: "Claude Code",
    descriptionKey: "tool.claude-code.description",
    status: "installed",
    version: "1.0.0",
    latestVersion: null,
    capabilities,
    sessionsInsideSettings: false,
    environment: "macos",
    ...overrides,
  };
}
function context(id = "claude-code") {
  return {
    tool: id,
    liveConfigPaths: [],
    resources: [
      {
        id: "instructions",
        kind: "instructions",
        scope: "global",
        path: id + "/AGENTS.md",
        exists: true,
        action: "edit",
        sizeBytes: 128,
        measurementLimited: false,
      },
    ],
    storage: {
      totalBytes: 0,
      sessionBytes: 0,
      sessionCount: 0,
      measurementLimited: false,
    },
    effectiveConnection: null,
  };
}
function failedRead() {
  return HttpResponse.text(
    JSON.stringify({
      code: "INTERNAL",
      messageKey: "error.tools.listFailed",
      technicalMessage: null,
      remediation: null,
      contextId: null,
    }),
    { status: 500 },
  );
}
function mountSessions() {
  const client = createTestQueryClient();
  const view = render(<DataPage />, { wrapper: withQueryClient(client) });
  return { ...view, client };
}
function mountDetails(value = tool(), onOpenExtensions = vi.fn()) {
  return render(
    <ToolDetailsModal
      tool={value}
      onClose={vi.fn()}
      onOpenExtensions={onOpenExtensions}
    />,
    { wrapper: withQueryClient(createTestQueryClient()) },
  );
}
describe("Sessions destination and relocated software resources", () => {
  beforeEach(async () => {
    i18n.addResourceBundle("en", "translation", en, true, true);
    await i18n.changeLanguage("en");
    server.use(
      http.post(endpoint + "/app_tools_list", () =>
        HttpResponse.json([tool(), tool({ id: "codex", name: "Codex" })]),
      ),
      http.post(endpoint + "/app_tools_check_versions", () =>
        HttpResponse.json([]),
      ),
      http.post(endpoint + "/app_sessions_list", () =>
        HttpResponse.json({ items: [], totalCount: 0, limited: false }),
      ),
      http.post(
        endpoint + "/app_provider_runtime_context",
        async ({ request }) =>
          HttpResponse.json(
            context(((await request.json()) as { tool: string }).tool),
          ),
      ),
    );
  });

  it("keeps one h1 and sessions without the relocated storage section", async () => {
    mountSessions();
    expect(screen.getAllByRole("heading", { level: 1 })).toHaveLength(1);
    expect(
      await screen.findByRole("region", { name: en.sessions.title }),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.data.storage.title)).toBeNull();
  });

  it("keeps every installed tool visible without confusing provider support with session support", async () => {
    server.use(
      http.post(endpoint + "/app_tools_list", () =>
        HttpResponse.json([
          tool(),
          tool({
            id: "kimi-code",
            name: "Kimi",
            capabilities: { ...capabilities, canManageProvider: false },
          }),
        ]),
      ),
    );
    mountSessions();
    const tabs = await screen.findByRole("tablist");
    expect(within(tabs).getAllByRole("tab")).toHaveLength(3);
    expect(within(tabs).getByRole("tab", { name: "Kimi" })).toBeInTheDocument();
    expect(screen.queryByText(en.data.scope.unsupportedShort)).toBeNull();
  });

  it("drives only the session list from a pill click and never requests resource scans", async () => {
    const requests: unknown[] = [];
    const runtime = vi.fn(() => HttpResponse.json(context()));
    server.use(
      http.post(endpoint + "/app_sessions_list", async ({ request }) => {
        requests.push(await request.json());
        return HttpResponse.json({ items: [], totalCount: 0, limited: false });
      }),
      http.post(endpoint + "/app_provider_runtime_context", runtime),
    );
    mountSessions();
    await waitFor(() =>
      expect(requests).toContainEqual({ query: null, tool: null }),
    );
    await userEvent.click(await screen.findByRole("tab", { name: "Codex" }));
    await waitFor(() =>
      expect(requests).toContainEqual({ query: null, tool: "codex" }),
    );
    await userEvent.click(screen.getByRole("tab", { name: "All" }));
    expect(screen.getByRole("tab", { name: "All" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(runtime).not.toHaveBeenCalled();
  });

  it("opens the selected software resources immediately and swaps them with the software", async () => {
    const view = mountDetails();
    expect(
      await screen.findByText("claude-code/AGENTS.md"),
    ).toBeInTheDocument();
    view.rerender(
      <ToolDetailsModal
        tool={tool({ id: "codex", name: "Codex" })}
        onClose={vi.fn()}
      />,
    );
    expect(await screen.findByText("codex/AGENTS.md")).toBeInTheDocument();
    expect(screen.queryByText("claude-code/AGENTS.md")).toBeNull();
  });

  it("hands global prompts to the Prompt library instead of opening a second write path", async () => {
    const opened = vi.fn();
    mountDetails(tool(), opened);
    await screen.findByText("claude-code/AGENTS.md");
    expect(
      screen.queryByRole("button", { name: en.services.runtime.action.edit }),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: en.services.runtime.action.manage }),
    );
    expect(opened).toHaveBeenCalledWith(
      { kind: "tool", id: "claude-code" },
      "prompt",
    );
  });

  it("keeps the direct editor for a tool whose global prompt the product cannot manage", async () => {
    mountDetails(
      tool({ capabilities: { ...capabilities, canManagePrompts: false } }),
    );
    await screen.findByText("claude-code/AGENTS.md");
    expect(
      screen.getByRole("button", { name: en.services.runtime.action.edit }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: en.services.runtime.action.manage }),
    ).toBeNull();
  });

  it("says it is still measuring rather than reporting zero before resources load", async () => {
    let release: (() => void) | undefined;
    server.use(
      http.post(endpoint + "/app_provider_runtime_context", async () => {
        await new Promise<void>((resolve) => {
          release = resolve;
        });
        return HttpResponse.json(context());
      }),
    );
    mountDetails();
    expect(
      await screen.findByText(en.services.runtime.loading),
    ).toBeInTheDocument();
    expect(screen.queryByText("0 B")).toBeNull();
    await waitFor(() => expect(release).toBeDefined());
    release?.();
    await screen.findByText("claude-code/AGENTS.md");
    expect(screen.getAllByText("0 B")).toHaveLength(2);
  });

  it("explains a failed tool read while sessions remain available and recovers on retry", async () => {
    let reads = 0;
    server.use(
      http.post(endpoint + "/app_tools_list", () =>
        ++reads === 1 ? failedRead() : HttpResponse.json([tool()]),
      ),
    );
    mountSessions();
    const alert = await screen.findByRole("alert");
    expect(
      within(alert).getByText(en.sessions.inventoryError),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("region", { name: en.sessions.title }),
    ).toBeInTheDocument();
    await userEvent.click(
      within(alert).getByRole("button", { name: en.data.inventory.retry }),
    );
    await screen.findByRole("tab", { name: "Claude Code" });
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("keeps the last tool list and says so when refresh fails", async () => {
    const { client } = mountSessions();
    await screen.findByRole("tab", { name: "Codex" });
    server.use(http.post(endpoint + "/app_tools_list", failedRead));
    await client.invalidateQueries({ queryKey: toolKeys.all });
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Codex" })).toBeInTheDocument();
  });

  it("marks aggregate totals as a lower bound when one tool cannot be measured", async () => {
    server.use(
      http.post(
        endpoint + "/app_provider_runtime_context",
        async ({ request }) => {
          const { tool: id } = (await request.json()) as { tool: string };
          return id === "codex"
            ? failedRead()
            : HttpResponse.json({
                ...context(),
                storage: {
                  totalBytes: 2048,
                  sessionBytes: 1024,
                  sessionCount: 1,
                  measurementLimited: false,
                },
              });
        },
      ),
    );
    render(
      <DataOverviewStrip
        manageable={[tool(), tool({ id: "codex" })]}
        inventory="ready"
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    expect(await screen.findByText("≥ 2 KB")).toBeInTheDocument();
    expect(screen.getByText("≥ 1 KB")).toBeInTheDocument();
    expect(screen.getByText(en.data.overview.partial)).toBeInTheDocument();
  });

  it("drops back to All when the selected software leaves the inventory", async () => {
    const { client } = mountSessions();
    await userEvent.click(await screen.findByRole("tab", { name: "Codex" }));
    server.use(
      http.post(endpoint + "/app_tools_list", () =>
        HttpResponse.json([tool()]),
      ),
    );
    await client.invalidateQueries({ queryKey: toolKeys.all });
    await waitFor(() =>
      expect(screen.queryByRole("tab", { name: "Codex" })).toBeNull(),
    );
    expect(screen.getByRole("tab", { name: "All" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByRole("tab", { name: "All" })).toHaveAttribute(
      "tabindex",
      "0",
    );
  });

  it("explains unsupported software details without reading runtime files", async () => {
    const runtime = vi.fn(() => HttpResponse.json(context()));
    server.use(http.post(endpoint + "/app_provider_runtime_context", runtime));
    mountDetails(
      tool({ capabilities: { ...capabilities, canManageProvider: false } }),
    );
    expect(
      screen.getByText(
        en.data.scope.unsupportedDescription.replace("{{tool}}", "Claude Code"),
      ),
    ).toBeInTheDocument();
    expect(runtime).not.toHaveBeenCalled();
  });

  it("keeps configuration files accessible in software details", async () => {
    server.use(
      http.post(endpoint + "/app_provider_runtime_context", () =>
        HttpResponse.json({
          ...context(),
          resources: [
            {
              ...context().resources[0],
              id: "config",
              kind: "configuration",
              path: "tool/config.json",
            },
          ],
        }),
      ),
    );
    mountDetails();
    expect(await screen.findByText("tool/config.json")).toBeInTheDocument();
    expect(
      screen.getByText(en.services.runtime.resource.configuration.title),
    ).toBeInTheDocument();
  });
});
