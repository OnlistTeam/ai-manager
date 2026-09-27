import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { routingKeys, type RoutingTraceSnapshot } from "@/entities/routing";
import en from "@/i18n/locales/en.json";
import { LiveRoutingPanel, maskEmails } from "@/features/live-routing";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const HIDE_EMAILS_KEY = "aimanager.liveRouting.hideEmails";

function provider(id: string, name: string, healthy = true) {
  return {
    id,
    name,
    priority: 1,
    current: true,
    healthy,
    consecutiveFailures: healthy ? 0 : 3,
  };
}

const alice = provider("alice", "alice.smith@example.com");

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
      currentProvider: alice,
      queue: [
        alice,
        provider("bob", "bob@corp.example"),
        provider("backup", "Backup API", false),
      ],
      available: [],
      unavailable: null,
      pickup: "live",
    },
    ...["codex", "gemini-cli", "grok-build"].map((tool) => ({
      tool,
      takeoverEnabled: false,
      autoFailoverEnabled: false,
      currentProvider: null,
      queue: [],
      available: [],
      unavailable: "noService",
      pickup: "atStart",
    })),
  ],
};

function attempt(
  id: string,
  name: string,
  outcome: "pending" | "ok" | "failed" | "skipped",
  error: string | null = null,
  httpStatus: number | null = null,
) {
  return {
    providerId: id,
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
    attempt("bob", "bob@corp.example", "failed", "rateLimited", 429),
    attempt("backup", "Backup API", "ok"),
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
  attempts: [attempt("only", "Only API", "failed", "timeout")],
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
  const client = createTestQueryClient();
  const view = render(<LiveRoutingPanel />, {
    wrapper: withQueryClient(client),
  });
  return { ...view, client };
}

function pushEntry(
  client: ReturnType<typeof createTestQueryClient>,
  entry: object,
) {
  act(() => {
    client.setQueryData<RoutingTraceSnapshot>(routingKeys.trace(), (current) =>
      current
        ? {
            ...current,
            revision: 50,
            entries: [
              entry as RoutingTraceSnapshot["entries"][number],
              ...current.entries,
            ],
          }
        : current,
    );
  });
}

function preferReducedMotion(reduce: boolean) {
  vi.spyOn(window, "matchMedia").mockImplementation(
    (query: string) =>
      ({
        matches: reduce && query.includes("reduce"),
        media: query,
        onchange: null,
        addListener: () => {},
        removeListener: () => {},
        addEventListener: () => {},
        removeEventListener: () => {},
        dispatchEvent: () => false,
      }) as unknown as MediaQueryList,
  );
}

const stage = () =>
  screen.getByRole("group", { name: en.routing.live.stage.label });
const services = () =>
  within(stage()).getByRole("list", {
    name: en.routing.live.stage.servicesLabel,
  });

describe("LiveRoutingPanel", () => {
  beforeEach(async () => {
    window.localStorage.removeItem(HIDE_EMAILS_KEY);
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, routing: en.routing },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("masks every email address, keeping the first letter and the domain", () => {
    expect(maskEmails("alice.smith@example.com")).toBe("a***@example.com");
    expect(maskEmails("Team (bo@x.io) and c@d.co.uk")).toBe(
      "Team (b***@x.io) and c***@d.co.uk",
    );
    expect(maskEmails("Backup API")).toBe("Backup API");
  });

  it("waits with the tools, AI Manager and a dashed place for services", async () => {
    mount([]);
    expect(await screen.findByText(en.routing.live.empty)).toBeInTheDocument();
    expect(
      within(stage()).getByText(en.routing.live.stage.noRequests),
    ).toBeInTheDocument();
    expect(
      within(stage()).getByText(en.routing.live.stage.yourTools),
    ).toBeInTheDocument();
    expect(
      within(stage()).getByText(en.routing.live.stage.hub),
    ).toBeInTheDocument();
    expect(screen.getByText("127.0.0.1:15721")).toBeInTheDocument();
    expect(screen.queryByRole("list")).toBeNull();
    expect(document.querySelector("circle[data-flight-dot]")).toBeNull();
  });

  it("shows the latest tool's services in routing order with how each try went", async () => {
    const view = mount([rerouted, failed]);
    const rows = await within(
      await screen.findByRole("list", { name: en.routing.live.listLabel }),
    ).findAllByRole("listitem");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute("data-trace-seq", "2");
    const first = within(rows[0]!);
    expect(first.getByText("Claude Code")).toBeInTheDocument();
    expect(first.getByText("claude-sonnet-5")).toBeInTheDocument();
    expect(
      first.getByText("b***@corp.example: failed (Rate limited · HTTP 429)"),
    ).toBeInTheDocument();
    expect(first.getByText("Backup API: answered")).toBeInTheDocument();
    expect(
      within(rows[1]!).getByText(en.routing.live.unknownModel),
    ).toBeInTheDocument();

    const serviceRows = await within(services()).findAllByRole("listitem");
    expect(serviceRows.map((row) => row.getAttribute("data-endpoint"))).toEqual(
      ["alice", "bob", "backup"],
    );
    expect(
      serviceRows.map(
        (row) =>
          row
            .querySelector("[data-endpoint-mark]")
            ?.getAttribute("data-endpoint-mark") ?? null,
      ),
    ).toEqual([null, "failed", "ok"]);
    expect(serviceRows[0]).toHaveTextContent("a***@example.com");
    expect(serviceRows[1]).toHaveTextContent(
      en.routing.live.errorCategory.rateLimited,
    );

    expect(
      view.container.querySelector("[data-live-caption]"),
    ).toHaveTextContent("Backup API answered in 1.3s");
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
      view.container.querySelector('[data-stage-tool="claude-code"]'),
    ).toBeInTheDocument();
    expect(
      view.container.querySelector('[data-stage-tool="codex"]'),
    ).toBeInTheDocument();
    expect(view.container.textContent).not.toMatch(/alice\.smith@|bob@corp/);
  });

  it("shows full addresses when email masking is turned off, and remembers it", async () => {
    const user = userEvent.setup();
    const view = mount([rerouted]);
    const toggle = await screen.findByRole("button", {
      name: en.routing.live.header.hideEmails,
    });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    await screen.findByText("a***@example.com");

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(window.localStorage.getItem(HIDE_EMAILS_KEY)).toBe("false");
    expect(screen.getByText("alice.smith@example.com")).toBeInTheDocument();
    expect(
      screen.getByText("bob@corp.example: failed (Rate limited · HTTP 429)"),
    ).toBeInTheDocument();

    view.unmount();
    mount([rerouted]);
    expect(
      await screen.findByRole("button", {
        name: en.routing.live.header.hideEmails,
      }),
    ).toHaveAttribute("aria-pressed", "false");
    expect(
      await screen.findByText("alice.smith@example.com"),
    ).toBeInTheDocument();
  });

  it("puts a dot on the stage for a request that arrives while it is open", async () => {
    preferReducedMotion(false);
    const { client, container } = mount([failed]);
    await screen.findByRole("list", { name: en.routing.live.listLabel });
    expect(container.querySelector("circle[data-flight-dot]")).toBeNull();

    pushEntry(client, {
      ...rerouted,
      seq: 3,
      revision: 50,
      attempts: [attempt("alice", "alice.smith@example.com", "pending")],
      status: "pending",
      totalMs: null,
      failedOver: false,
    });
    await waitFor(() =>
      expect(
        container.querySelector('circle[data-flight-dot="3"]'),
      ).toBeInTheDocument(),
    );
  });

  it("changes only state, without travelling dots, under reduced motion", async () => {
    preferReducedMotion(true);
    const { client, container } = mount([failed]);
    await screen.findByRole("list", { name: en.routing.live.listLabel });

    pushEntry(client, {
      ...rerouted,
      seq: 3,
      revision: 50,
      attempts: [attempt("alice", "alice.smith@example.com", "pending")],
      status: "pending",
      totalMs: null,
      failedOver: false,
    });
    await waitFor(() =>
      expect(container.querySelector("[data-live-caption]")).toHaveTextContent(
        "Claude Code sent a request to a***@example.com…",
      ),
    );
    expect(container.querySelector("circle[data-flight-dot]")).toBeNull();
    expect(
      container.querySelector('[data-endpoint="alice"] [data-endpoint-mark]'),
    ).toHaveAttribute("data-endpoint-mark", "pending");
    // The wire the request waits on is marked for its steady pulse.
    expect(
      container
        .querySelector('path[data-wire="endpoint:alice"]')
        ?.hasAttribute("data-held"),
    ).toBe(true);
    expect(
      container
        .querySelector('path[data-wire="endpoint:bob"]')
        ?.hasAttribute("data-held"),
    ).toBe(false);
  });
});
