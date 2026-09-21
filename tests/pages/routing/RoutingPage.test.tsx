import { render, screen, waitFor } from "@testing-library/react";
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

function overview(takeoverEnabled = false) {
  return {
    running: takeoverEnabled,
    address: takeoverEnabled ? "127.0.0.1" : null,
    port: takeoverEnabled ? 15_721 : null,
    activeConnections: 0,
    totalRequests: 12,
    successRequests: 11,
    failedRequests: 1,
    failoverCount: 1,
    targets: [
      {
        tool: "claude-code",
        takeoverEnabled,
        autoFailoverEnabled: false,
        currentProvider: provider("provider-a", "Primary API", 1, true),
        queue: [
          provider("provider-a", "Primary API", 1, true),
          provider("provider-b", "Backup API", 2),
        ],
        available: [provider("provider-c", "Third API", null)],
      },
      ...["codex", "gemini-cli", "grok-build"].map((tool) => ({
        tool,
        takeoverEnabled: false,
        autoFailoverEnabled: false,
        currentProvider: null,
        queue: [],
        available: [],
      })),
    ],
  };
}

function mount(response = overview()) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
      HttpResponse.json(response),
    ),
  );
  return render(<RoutingPage />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("RoutingPage", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, routing: en.routing },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("shows four supported targets, aggregate status and ordered failover services", async () => {
    mount();

    expect(
      await screen.findByText(en.routing.summary.inactive),
    ).toBeInTheDocument();
    expect(screen.getByText("Primary API")).toBeInTheDocument();
    expect(screen.getByText("Backup API")).toBeInTheDocument();
    expect(screen.getByText("P1")).toBeInTheDocument();
    expect(screen.getByText("P2")).toBeInTheDocument();
    expect(screen.getAllByRole("tab")).toHaveLength(4);
    expect(document.querySelectorAll("[data-routing-target]")).toHaveLength(1);
    expect(screen.getByRole("tab", { name: "Claude Code" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    await userEvent.click(screen.getByRole("tab", { name: "Codex" }));
    expect(screen.getByRole("tabpanel", { name: "Codex" })).toBeInTheDocument();
    expect(screen.queryByText("Primary API")).toBeNull();
    expect(document.querySelector("[data-routing-target]")).toHaveAttribute(
      "data-routing-target",
      "codex",
    );
    await userEvent.keyboard("{ArrowRight}");
    expect(
      screen.getByRole("tabpanel", { name: "Gemini CLI" }),
    ).toBeInTheDocument();
  });

  it("does not rewrite live configuration until takeover is confirmed", async () => {
    let calls = 0;
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_routing_set_takeover`,
        async ({ request }) => {
          calls += 1;
          body = await request.json();
          return HttpResponse.json(overview(true));
        },
      ),
    );
    mount();

    await userEvent.click(
      await screen.findByRole("switch", {
        name: "Routing takeover for Claude Code",
      }),
    );
    expect(calls).toBe(0);
    expect(
      screen.getByRole("dialog", { name: "Route Claude Code locally?" }),
    ).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Turn on routing" }),
    );

    await waitFor(() => expect(calls).toBe(1));
    expect(body).toEqual({ tool: "claude-code", enabled: true });
    expect(
      await screen.findByRole("switch", {
        name: "Routing takeover for Claude Code",
      }),
    ).toBeChecked();
  });

  it("hot-switches through the product API and can add another failover service", async () => {
    const requests: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_routing_switch_provider`,
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json(overview(true));
        },
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_routing_queue_add`,
        async ({ request }) => {
          requests.push(await request.json());
          return HttpResponse.json(overview(true));
        },
      ),
    );
    mount(overview(true));

    await userEvent.click(
      await screen.findByRole("button", {
        name: "Switch routing to Backup API",
      }),
    );
    await userEvent.selectOptions(
      screen.getByRole("combobox", {
        name: "Service to add to Claude Code failover",
      }),
      "provider-c",
    );
    await userEvent.click(
      screen.getByRole("button", {
        name: "Add the selected service to Claude Code failover",
      }),
    );

    await waitFor(() => expect(requests).toHaveLength(2));
    expect(requests).toEqual([
      { tool: "claude-code", providerId: "provider-b" },
      { tool: "claude-code", providerId: "provider-c" },
    ]);
  });

  it("keeps stop-and-restore behind confirmation in the compact summary", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_stop_all`, () => {
        calls += 1;
        return HttpResponse.json(overview());
      }),
    );
    mount(overview(true));
    await userEvent.click(
      await screen.findByRole("button", { name: en.routing.stop.action }),
    );
    expect(calls).toBe(0);
    expect(
      screen.getByRole("dialog", { name: en.routing.stop.title }),
    ).toBeInTheDocument();
    expect(screen.getByText(en.routing.stop.description)).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: en.routing.stop.confirm }),
    );
    await waitFor(() => expect(calls).toBe(1));
    expect(
      await screen.findByText(en.routing.summary.inactive),
    ).toBeInTheDocument();
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
