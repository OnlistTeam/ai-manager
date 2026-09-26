import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { HomeLiveRouting } from "@/pages/home/HomeLiveRouting";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function overview(routed: boolean) {
  return {
    running: routed,
    address: routed ? "127.0.0.1" : null,
    port: routed ? 15_721 : null,
    activeConnections: 0,
    totalRequests: 0,
    successRequests: 0,
    failedRequests: 0,
    failoverCount: 0,
    targets: [
      {
        tool: "claude-code",
        takeoverEnabled: routed,
        autoFailoverEnabled: false,
        currentProvider: {
          id: "claude-service",
          name: "Service",
          priority: null,
          current: true,
          healthy: true,
          consecutiveFailures: 0,
        },
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
}

function mount(routed: boolean, onOpenPrivacySettings = vi.fn()) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
      HttpResponse.json(overview(routed)),
    ),
    http.post(`${TAURI_ENDPOINT}/app_routing_trace`, () =>
      HttpResponse.json({
        revision: 0,
        counts: { requests: 0, rerouted: 0, failed: 0 },
        entries: [],
      }),
    ),
  );
  render(<HomeLiveRouting onOpenPrivacySettings={onOpenPrivacySettings} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("HomeLiveRouting", () => {
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

  it("shows only the switch while live routing is off", async () => {
    mount(false);

    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: en.routing.live.title }),
      ).toBeEnabled(),
    );
    expect(screen.queryByText(en.routing.live.empty)).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: en.routing.privacy.openSettingsLabel,
      }),
    ).not.toBeInTheDocument();
  });

  it("says what privacy protection hides and links to its settings while it is on", async () => {
    const onOpenPrivacySettings = vi.fn();
    mount(true, onOpenPrivacySettings);

    expect(await screen.findByText(en.routing.live.empty)).toBeInTheDocument();
    expect(
      await screen.findByText(
        "Privacy: keys and passwords hidden · personal information not hidden",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("switch", { name: /privacy/i })).toBeNull();

    await userEvent.click(
      screen.getByRole("button", {
        name: en.routing.privacy.openSettingsLabel,
      }),
    );
    expect(onOpenPrivacySettings).toHaveBeenCalledTimes(1);
  });
});
