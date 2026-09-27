import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { RoutingPage } from "@/pages/routing/RoutingPage";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function provider(
  id: string,
  name: string,
  priority: number | null,
  current = false,
) {
  return {
    id,
    name,
    priority,
    current,
    healthy: true,
    consecutiveFailures: 0,
  };
}

/** Claude Code (rereads its settings) can be routed; Codex (reads them at
 * start) too; Gemini CLI signs in with its own account; Grok Build has no
 * endpoint. */
function overview(routed: { claude?: boolean; codex?: boolean } = {}) {
  const running = Boolean(routed.claude || routed.codex);
  return {
    running,
    address: running ? "127.0.0.1" : null,
    port: running ? 15_721 : null,
    activeConnections: 0,
    totalRequests: 12,
    successRequests: 11,
    failedRequests: 1,
    failoverCount: 1,
    targets: [
      {
        tool: "claude-code",
        takeoverEnabled: Boolean(routed.claude),
        autoFailoverEnabled: Boolean(routed.claude),
        currentProvider: provider("provider-a", "Primary API", 1, true),
        queue: [
          provider("provider-a", "Primary API", 1, true),
          provider("provider-b", "Backup API", 2),
        ],
        available: [provider("provider-c", "Third API", null)],
        unavailable: null,
        pickup: "live",
      },
      {
        tool: "codex",
        takeoverEnabled: Boolean(routed.codex),
        autoFailoverEnabled: false,
        currentProvider: provider("relay", "Team Relay", null, true),
        queue: [],
        available: [],
        unavailable: null,
        pickup: "atStart",
      },
      {
        tool: "gemini-cli",
        takeoverEnabled: false,
        autoFailoverEnabled: false,
        currentProvider: provider("google", "Google Login", null, true),
        queue: [],
        available: [],
        unavailable: "ownLogin",
        pickup: "atStart",
      },
      {
        tool: "grok-build",
        takeoverEnabled: false,
        autoFailoverEnabled: false,
        currentProvider: null,
        queue: [],
        available: [],
        unavailable: "noService",
        pickup: "atStart",
      },
    ],
  };
}

const EMPTY_TRACE = {
  revision: 0,
  counts: { requests: 0, rerouted: 0, failed: 0 },
  entries: [],
};

function mount(response: object = overview()) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
      HttpResponse.json(response),
    ),
    http.post(`${TAURI_ENDPOINT}/app_routing_trace`, () =>
      HttpResponse.json(EMPTY_TRACE),
    ),
  );
  return render(<RoutingPage />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

function row(name: string): HTMLElement {
  return screen.getByRole("article", { name });
}

function routeSwitch(name: string): HTMLElement {
  return screen.getByRole("switch", {
    name: `Route ${name} through AI Manager`,
  });
}

describe("RoutingPage", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, nav: en.nav, routing: en.routing },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("lists every tool on one line with its endpoint, switch or reason", async () => {
    mount();

    expect(
      await screen.findByText(en.routing.summary.inactive),
    ).toBeInTheDocument();
    expect(screen.getAllByRole("article")).toHaveLength(4);
    expect(within(row("Claude Code")).getByText("Primary API")).toBeVisible();
    expect(routeSwitch("Claude Code")).not.toBeChecked();
    expect(routeSwitch("Codex")).not.toBeChecked();
    // Nothing is taken over by default, and a connection that cannot be
    // forwarded shows why instead of a switch.
    expect(
      within(row("Gemini CLI")).getByText(en.routing.unavailable.ownLogin),
    ).toBeVisible();
    expect(within(row("Gemini CLI")).queryByRole("switch")).toBeNull();
    expect(
      within(row("Grok Build")).getByText(en.routing.unavailable.noService),
    ).toBeVisible();
    expect(within(row("Grok Build")).queryByRole("switch")).toBeNull();
    // No failover controls and no live panel while nothing is routed.
    expect(screen.queryByText(en.routing.failover.title)).toBeNull();
    expect(
      screen.queryByRole("region", { name: en.routing.live.panelLabel }),
    ).toBeNull();
  });

  it("routes one tool from its own switch and says what open sessions do", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_routing_set_takeover`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json(overview({ codex: true }));
        },
      ),
    );
    mount();

    await userEvent.click(
      await screen.findByRole("switch", {
        name: "Route Codex through AI Manager",
      }),
    );

    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() =>
      expect(bodies).toEqual([{ tool: "codex", enabled: true }]),
    );
    expect(await within(row("Codex")).findByRole("status")).toHaveTextContent(
      "Open Codex sessions keep their direct connection until you restart them.",
    );
    expect(routeSwitch("Codex")).toBeChecked();
    expect(routeSwitch("Claude Code")).not.toBeChecked();
    expect(
      await screen.findByRole("region", { name: en.routing.live.panelLabel }),
    ).toBeInTheDocument();
  });

  it("turns a tool that rereads its settings off with the honest note", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_set_takeover`, () =>
        HttpResponse.json({
          ...overview(),
          running: true,
          address: "127.0.0.1",
          port: 15_721,
        }),
      ),
    );
    mount(overview({ claude: true }));

    await userEvent.click(
      await screen.findByRole("switch", {
        name: "Route Claude Code through AI Manager",
      }),
    );

    expect(
      await within(row("Claude Code")).findByRole("status"),
    ).toHaveTextContent(
      "Open Claude Code sessions switch back right away, but keep any setting routing added until you restart them.",
    );
    // The gateway keeps running for sessions that still hold its address.
    expect(
      screen.getByRole("region", { name: en.routing.live.panelLabel }),
    ).toBeInTheDocument();
  });

  it("hot-switches through the product API and can add another failover service", async () => {
    const requests: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_routing_switch_provider`,
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json(overview({ claude: true }));
        },
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_routing_queue_add`,
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json(overview({ claude: true }));
        },
      ),
    );
    mount(overview({ claude: true }));

    expect(
      await screen.findByRole("switch", {
        name: "Automatic failover for Claude Code",
      }),
    ).toBeChecked();
    await userEvent.click(
      screen.getByRole("button", { name: "Switch routing to Backup API" }),
    );
    await userEvent.selectOptions(
      screen.getByRole("combobox", {
        name: "Endpoint to add to Claude Code failover",
      }),
      "provider-c",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: "Add the selected endpoint to Claude Code failover",
      }),
    );

    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests).toEqual([
      { tool: "claude-code", providerId: "provider-b" },
      { tool: "claude-code", providerId: "provider-c" },
    ]);
  });

  it("shows why a routed tool's new endpoint cannot be forwarded", async () => {
    const base = overview({ claude: true });
    mount({
      ...base,
      targets: [
        { ...base.targets[0], unavailable: "ownLogin" },
        ...base.targets.slice(1),
      ],
    });

    const claude = await screen.findByRole("article", { name: "Claude Code" });
    expect(within(claude).getByRole("alert")).toHaveTextContent(
      en.routing.row.routedButUnavailable,
    );
    expect(routeSwitch("Claude Code")).toBeChecked();
  });

  it("keeps backend details out of a retryable read error", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
        HttpResponse.text("/Users/alice/.config token=private", {
          status: 500,
        }),
      ),
    );
    render(<RoutingPage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect(
      await screen.findByText("Could not read local routing"),
    ).toBeInTheDocument();
    expect(screen.queryByText(/alice|token=private/i)).toBeNull();
  });
});
