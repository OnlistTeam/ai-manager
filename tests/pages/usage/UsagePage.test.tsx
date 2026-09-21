import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
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

function mount(response = overview()) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
      HttpResponse.json(response),
    ),
  );
  return render(<UsagePage />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
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

  it("syncs only after the user asks and reports sanitized progress", async () => {
    let refreshCalls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () => {
        refreshCalls += 1;
        return HttpResponse.json({
          overview: overview(15),
          sync: {
            filesScanned: 4,
            recordsImported: 3,
            recordsSkipped: 1,
            sourceIssues: 0,
          },
        });
      }),
    );
    mount();

    const button = await screen.findByRole("button", {
      name: "Sync local usage",
    });
    expect(refreshCalls).toBe(0);
    await userEvent.click(button);

    await waitFor(() => expect(refreshCalls).toBe(1));
    expect(
      await screen.findByText("Scanned 4 local files and added 3 new records."),
    ).toBeInTheDocument();
  });

  it("makes a partial local scan visible without discarding the overview", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () =>
        HttpResponse.json({
          overview: overview(13),
          sync: {
            filesScanned: 2,
            recordsImported: 1,
            recordsSkipped: 0,
            sourceIssues: 2,
          },
        }),
      ),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: "Sync local usage" }),
    );

    expect(
      await screen.findByText(/2 source checks could not finish/),
    ).toBeInTheDocument();
    expect(screen.getAllByText("$1.25").length).toBeGreaterThan(0);
  });

  it("offers an explicit first sync when no records exist", async () => {
    mount(overview(0));

    expect(
      await screen.findByText("No local usage records yet"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Sync local usage" }),
    ).toBeInTheDocument();
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
        return HttpResponse.json({
          overview: overview(15),
          sync: {
            filesScanned: 4,
            recordsImported: 3,
            recordsSkipped: 1,
            sourceIssues: 0,
          },
        });
      }),
    );
    const wrapper = withQueryClient(createTestQueryClient());
    const first = render(<UsagePage />, { wrapper });
    await userEvent.click(
      await screen.findByRole("button", { name: "Sync local usage" }),
    );
    await waitFor(() => expect(calls).toBe(1));
    expect(
      screen.getByRole("status", { name: "Syncing local usage" }),
    ).toBeInTheDocument();
    first.unmount();
    render(<UsagePage />, { wrapper });
    const button = await screen.findByRole("button", {
      name: "Sync local usage",
    });
    expect(button).toBeDisabled();
    expect(
      screen.getByRole("status", { name: "Syncing local usage" }),
    ).toBeInTheDocument();
    await userEvent.click(button);
    expect(calls).toBe(1);
    finish();
    expect(
      await screen.findByText("Scanned 4 local files and added 3 new records."),
    ).toBeInTheDocument();
    expect(button).toBeEnabled();
    expect(
      screen.queryByRole("status", { name: "Syncing local usage" }),
    ).toBeNull();
  });

  it("ends the busy state on a failed sync and keeps the previous summary", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_refresh`, () =>
        HttpResponse.json(
          {
            code: "UPSTREAM_ERROR",
            messageKey: "error.usage.readFailed",
            technicalMessage: "private path",
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    const button = await screen.findByRole("button", {
      name: "Sync local usage",
    });
    await userEvent.click(button);
    expect(
      await screen.findByText(en.usage.sync.errorTitle),
    ).toBeInTheDocument();
    expect(button).toBeEnabled();
    expect(screen.getAllByText("$1.25").length).toBeGreaterThan(0);
    expect(screen.queryByText("private path")).toBeNull();
  });

  it("shows a retryable read error without rendering backend details", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_usage_overview`, () =>
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
      screen.getByRole("button", { name: "Try again" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/alice|session\.jsonl/i)).toBeNull();
  });
});
