import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { USAGE_AUTO_SYNC_GAP_MS } from "@/entities/usage";
import en from "@/i18n/locales/en.json";
import { UsagePage } from "@/pages/usage/UsagePage";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function metrics(requests = 12) {
  return {
    requests,
    estimatedCostUsd: "1.250000",
    tokens: requests === 0 ? 0 : 25_000,
    successRatePercent: requests === 0 ? 0 : 91.5,
    cacheHitRatePercent: requests === 0 ? 0 : 42,
  };
}

function overview(requests = 12) {
  return {
    periodDays: 30,
    startDate: "2026-07-26",
    endDate: "2026-08-24",
    summary: metrics(requests),
    byTool:
      requests === 0 ? [] : [{ tool: "codex", metrics: metrics(requests) }],
    trend:
      requests === 0
        ? []
        : [
            {
              date: "2026-08-24",
              requests,
              estimatedCostUsd: "1.250000",
              tokens: 25_000,
            },
          ],
  };
}

function syncResult(requests = 15, sourceIssues = 0) {
  return {
    overview: overview(requests),
    sync: {
      filesScanned: 4,
      recordsImported: 3,
      recordsSkipped: 1,
      sourceIssues,
    },
  };
}

function mount(response = overview(), synced = syncResult()) {
  let syncs = 0;
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
      HttpResponse.json(response),
    ),
    http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () => {
      syncs += 1;
      return HttpResponse.json(synced);
    }),
  );
  const client = createTestQueryClient();
  const view = render(<UsagePage />, { wrapper: withQueryClient(client) });
  return { ...view, client, syncs: () => syncs };
}

describe("UsagePage", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { error: en.error, usage: en.usage },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  afterEach(() => vi.restoreAllMocks());

  it("renders the aggregate summary, trend, privacy boundary and tool breakdown", async () => {
    mount();

    expect((await screen.findAllByText("$1.25")).length).toBeGreaterThan(0);
    expect(screen.getByText("25K")).toBeInTheDocument();
    expect(screen.getByText("Daily token use")).toBeInTheDocument();
    expect(screen.getByText("Codex")).toBeInTheDocument();
    expect(
      screen.getByText("Aggregate and local by design"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/private prompt|sourcePath|\/Users\/alice/i),
    ).toBeNull();
  });

  it("syncs on its own when opened and offers no sync button", async () => {
    const view = mount();

    await waitFor(() => expect(view.syncs()).toBe(1));
    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "Syncing local usage" }),
      ).toBeNull(),
    );
    expect(screen.getAllByText("$1.25").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByText(/Scanned 4 local files/)).toBeNull();
  });

  it("marks a sync beside the title while the last summary stays visible", async () => {
    let finish!: () => void;
    const waiting = new Promise<void>((resolve) => {
      finish = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json(overview()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, async () => {
        await waiting;
        return HttpResponse.json(syncResult());
      }),
    );
    render(<UsagePage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect((await screen.findAllByText("$1.25")).length).toBeGreaterThan(0);
    const indicator = await screen.findByRole("status", {
      name: "Syncing local usage",
    });
    expect(indicator).toHaveTextContent("Syncing local usage");
    expect(screen.queryByText(en.usage.sync.runningHint)).toBeNull();
    finish();
    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "Syncing local usage" }),
      ).toBeNull(),
    );
  });

  it("syncs again when the window comes back, but not twice in a row", async () => {
    const view = mount();
    await waitFor(() => expect(view.syncs()).toBe(1));
    await waitFor(() =>
      expect(
        screen.queryByRole("status", { name: "Syncing local usage" }),
      ).toBeNull(),
    );

    fireEvent.focus(window);
    expect(view.client.isMutating()).toBe(0);

    const later = Date.now() + USAGE_AUTO_SYNC_GAP_MS + 1_000;
    vi.spyOn(Date, "now").mockReturnValue(later);
    fireEvent.focus(window);
    await waitFor(() => expect(view.syncs()).toBe(2));
  });

  it("makes a partial local scan visible without discarding the overview", async () => {
    mount(overview(), syncResult(13, 2));

    expect(
      await screen.findByText(/2 source checks could not finish/),
    ).toBeInTheDocument();
    expect(screen.getAllByText("$1.25").length).toBeGreaterThan(0);
  });

  it("shows the first import instead of an empty page while it runs", async () => {
    let finish!: () => void;
    const waiting = new Promise<void>((resolve) => {
      finish = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json(overview(0)),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, async () => {
        await waiting;
        return HttpResponse.json(syncResult(0));
      }),
    );
    render(<UsagePage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect(
      await screen.findByText(en.usage.sync.runningHint),
    ).toBeInTheDocument();
    expect(screen.queryByText(en.usage.empty.title)).toBeNull();
    finish();
    expect(await screen.findByText(en.usage.empty.title)).toBeInTheDocument();
    expect(screen.getByText(en.usage.empty.description)).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("keeps an in-flight sync across navigation and does not enqueue duplicates", async () => {
    let finish!: () => void;
    const waiting = new Promise<void>((resolve) => {
      finish = resolve;
    });
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json(overview()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, async () => {
        calls += 1;
        await waiting;
        return HttpResponse.json(syncResult(13, 1));
      }),
    );
    const wrapper = withQueryClient(createTestQueryClient());
    const first = render(<UsagePage />, { wrapper });
    await waitFor(() => expect(calls).toBe(1));
    expect(
      await screen.findByRole("status", { name: "Syncing local usage" }),
    ).toBeInTheDocument();
    first.unmount();
    render(<UsagePage />, { wrapper });
    expect(
      await screen.findByRole("status", { name: "Syncing local usage" }),
    ).toBeInTheDocument();
    fireEvent.focus(window);
    expect(calls).toBe(1);
    finish();
    expect(
      await screen.findByText(/1 source checks could not finish/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("status", { name: "Syncing local usage" }),
    ).toBeNull();
    expect(calls).toBe(1);
  });

  it("offers a retry only after a failed sync and keeps the previous summary", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.json(overview()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () => {
        calls += 1;
        return HttpResponse.json(
          {
            code: "UPSTREAM_ERROR",
            messageKey: "error.usage.readFailed",
            technicalMessage: "private path",
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        );
      }),
    );
    render(<UsagePage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect(
      await screen.findByText(en.usage.sync.errorTitle),
    ).toBeInTheDocument();
    expect(screen.getAllByText("$1.25").length).toBeGreaterThan(0);
    expect(screen.queryByText("private path")).toBeNull();
    const retry = screen.getByRole("button", { name: "Try again" });
    await userEvent.click(retry);
    await waitFor(() => expect(calls).toBe(2));
  });

  it("shows a retryable read error without rendering backend details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
        HttpResponse.text("/Users/alice/private/session.jsonl", {
          status: 500,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () =>
        HttpResponse.text("/Users/alice/private/session.jsonl", {
          status: 500,
        }),
      ),
    );
    render(<UsagePage />, {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    expect(
      await screen.findByText("Could not read local usage"),
    ).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: "Try again" }).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByText(/alice|session\.jsonl/i)).toBeNull();
  });
});
