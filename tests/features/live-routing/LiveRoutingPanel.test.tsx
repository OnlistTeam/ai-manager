import { render, screen, within } from "@testing-library/react";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { LiveRoutingPanel, maskEmails } from "@/features/live-routing";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function provider(name: string) {
  return {
    id: name,
    name,
    priority: 1,
    current: true,
    healthy: true,
    consecutiveFailures: 0,
  };
}

const overview = {
  running: true,
  address: "127.0.0.1",
  port: 15_721,
  activeConnections: 0,
  totalRequests: 0,
  successRequests: 0,
  failedRequests: 0,
  failoverCount: 0,
  targets: [
    {
      tool: "claude-code",
      takeoverEnabled: true,
      autoFailoverEnabled: true,
      currentProvider: provider("alice.smith@example.com"),
      queue: [],
      available: [],
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

function attempt(
  name: string,
  outcome: "ok" | "failed" | "skipped",
  error: string | null = null,
  httpStatus: number | null = null,
) {
  return {
    providerId: name,
    providerName: name,
    outcome,
    httpStatus,
    error,
    ms: 90,
  };
}

const rerouted = {
  seq: 2,
  revision: 5,
  startedAt: Date.UTC(2026, 8, 26, 6, 30, 5),
  tool: "claude-code",
  model: "claude-sonnet-5",
  attempts: [
    attempt("bob@corp.example", "failed", "rateLimited", 429),
    attempt("Backup API", "ok"),
  ],
  status: "ok",
  error: null,
  totalMs: 1_250,
  failedOver: true,
};

const failed = {
  ...rerouted,
  seq: 1,
  revision: 3,
  tool: "codex",
  model: null,
  attempts: [attempt("Only API", "failed", "timeout")],
  status: "failed",
  error: "timeout",
  totalMs: 30_000,
  failedOver: false,
};

function mount(entries: unknown[]) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
      HttpResponse.json(overview),
    ),
    http.post(`${TAURI_ENDPOINT}/app_routing_trace`, () =>
      HttpResponse.json({
        revision: 5,
        counts: { requests: 42, rerouted: 3, failed: 1 },
        entries,
      }),
    ),
  );
  return render(<LiveRoutingPanel />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("LiveRoutingPanel", () => {
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

  it("masks every email address, keeping the first letter and the domain", () => {
    expect(maskEmails("alice.smith@example.com")).toBe("a***@example.com");
    expect(maskEmails("Team (bo@x.io) and c@d.co.uk")).toBe(
      "Team (b***@x.io) and c***@d.co.uk",
    );
    expect(maskEmails("Backup API")).toBe("Backup API");
  });

  it("shows one line when nothing has been routed yet", async () => {
    mount([]);
    expect(await screen.findByText(en.routing.live.empty)).toBeInTheDocument();
    expect(screen.queryByRole("list")).toBeNull();
  });

  it("shows the flow, the totals and each request on one line", async () => {
    const view = mount([rerouted, failed]);
    const rows = await screen.findAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute("data-trace-seq", "2");

    const first = within(rows[0]!);
    expect(first.getByText("Claude Code")).toBeInTheDocument();
    expect(first.getByText("claude-sonnet-5")).toBeInTheDocument();
    expect(first.getByText("b***@corp.example")).toBeInTheDocument();
    expect(
      first.getByText("b***@corp.example: failed (Rate limited · HTTP 429)"),
    ).toBeInTheDocument();
    expect(first.getByText("Backup API: answered")).toBeInTheDocument();
    expect(rows[0]!.querySelectorAll("[data-attempt-outcome]")).toHaveLength(2);

    const second = within(rows[1]!);
    expect(second.getByText(en.routing.live.unknownModel)).toBeInTheDocument();
    expect(
      second.getByText(en.routing.live.errorCategory.timeout),
    ).toBeInTheDocument();

    expect(await screen.findByText("a***@example.com")).toBeInTheDocument();
    expect(
      view.container.querySelector('[data-counter="requests"]'),
    ).toHaveTextContent("42");
    expect(
      view.container.querySelector('[data-counter="rerouted"]'),
    ).toHaveTextContent("3");
    expect(
      view.container.querySelector('[data-counter="failed"]'),
    ).toHaveTextContent("1");
    expect(
      view.container.querySelector('[data-flow-tool="claude-code"]'),
    ).toHaveAttribute("data-active", "true");
    expect(view.container.textContent).not.toMatch(/alice\.smith@|bob@corp/);
  });
});
