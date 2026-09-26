import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { LiveRoutingSwitch } from "@/features/live-routing";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const TOOLS = ["claude-code", "codex", "gemini-cli", "grok-build"] as const;

function overview(routed: readonly string[], ready: readonly string[]) {
  return {
    running: routed.length > 0,
    address: routed.length > 0 ? "127.0.0.1" : null,
    port: routed.length > 0 ? 15_721 : null,
    activeConnections: 0,
    totalRequests: 0,
    successRequests: 0,
    failedRequests: 0,
    failoverCount: 0,
    targets: TOOLS.map((tool) => ({
      tool,
      takeoverEnabled: routed.includes(tool),
      autoFailoverEnabled: false,
      currentProvider: ready.includes(tool)
        ? {
            id: `${tool}-service`,
            name: "Service",
            priority: null,
            current: true,
            healthy: true,
            consecutiveFailures: 0,
          }
        : null,
      queue: [],
      available: [],
    })),
  };
}

function mount(initial: ReturnType<typeof overview>) {
  server.use(
    http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
      HttpResponse.json(initial),
    ),
  );
  render(<LiveRoutingSwitch />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("LiveRoutingSwitch", () => {
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

  it("names the tools it will take over, and cancelling changes nothing", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_routing_set_live_mode`, () => {
        calls += 1;
        return HttpResponse.json({ overview: overview([], []), failures: [] });
      }),
    );
    mount(overview([], ["claude-code", "gemini-cli"]));

    await userEvent.click(
      await screen.findByRole("switch", { name: en.routing.live.title }),
    );
    expect(
      screen.getByRole("dialog", { name: en.routing.live.confirm.title }),
    ).toHaveTextContent(
      "Claude Code, Gemini CLI will send requests through AI Manager. Their settings are backed up first",
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.ds.action.cancel }),
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls).toBe(0);
  });

  it("reports the tools that could not be taken over after a partial success", async () => {
    const bodies: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_routing_set_live_mode`,
        async ({ request }) => {
          bodies.push(await request.json());
          return HttpResponse.json({
            overview: overview(["claude-code"], ["claude-code", "gemini-cli"]),
            failures: [
              {
                tool: "gemini-cli",
                error: {
                  code: "CONFIG_WRITE_FAILED",
                  messageKey: "error.routing.liveTakeoverFailed",
                  technicalMessage: "the Gemini .env file does not exist",
                  remediation: null,
                  contextId: null,
                },
              },
            ],
          });
        },
      ),
    );
    mount(overview([], ["claude-code", "gemini-cli"]));

    await userEvent.click(
      await screen.findByRole("switch", { name: en.routing.live.title }),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.routing.live.confirm.action }),
    );

    await waitFor(() => expect(bodies).toEqual([{ enabled: true }]));
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(en.routing.live.partial.title);
    expect(alert).toHaveTextContent(
      `Gemini CLI: ${en.error.routing.liveTakeoverFailed}`,
    );
    expect(alert).not.toHaveTextContent(".env");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(
      screen.getByRole("switch", { name: en.routing.live.title }),
    ).toBeChecked();
  });

  it("cannot start before any tool has a service", async () => {
    mount(overview([], []));
    const toggle = await screen.findByRole("switch", {
      name: en.routing.live.title,
    });
    await waitFor(() => expect(toggle).toBeDisabled());
    expect(
      screen.getByText(
        `Choose a service for a tool in ${en.nav.services} first.`,
      ),
    ).toBeInTheDocument();
  });
});
