import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import en from "@/i18n/locales/en.json";
import { UpdateSection } from "@/features/updater/UpdateSection";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function wire(
  phase: "unconfigured" | "checking" | "downloading" | "ready" | "upToDate",
  attempt = phase === "unconfigured" ? 0 : 1,
) {
  return {
    currentVersion: "1.2.3",
    availableVersion:
      phase === "downloading" || phase === "ready" ? "1.3.0" : null,
    channelReady: phase !== "unconfigured",
    phase,
    downloadedBytes: phase === "downloading" ? 50 : phase === "ready" ? 100 : 0,
    totalBytes: phase === "downloading" || phase === "ready" ? 100 : null,
    attempt,
    maxAttempts: phase === "unconfigured" ? 0 : 3,
  };
}

function target(tool: string, routed: boolean) {
  return {
    tool,
    takeoverEnabled: routed,
    autoFailoverEnabled: false,
    currentProvider: null,
    queue: [],
    available: [],
    unavailable: null,
    pickup: "atStart",
  };
}

/** Claude Code routed or direct, the other tools direct (ADR-0054). */
function routingOverview(routed: boolean) {
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
      target("claude-code", routed),
      target("codex", false),
      target("gemini-cli", false),
      target("grok-build", false),
    ],
  };
}

function mount() {
  return render(<UpdateSection />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("UpdateSection", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("describes a build without a signed channel as a development build, not a fault", async () => {
    mount();

    expect(
      await screen.findByText(en.preferences.updates.devBuild),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.updates.devBuildNote),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
    expect(
      screen.queryByRole("button", { name: en.preferences.updates.check }),
    ).toBeNull();
    expect(screen.queryByText(/not configured/i)).toBeNull();
  });

  it("shows verified background-download progress without blocking the app", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
        HttpResponse.json(wire("downloading")),
      ),
    );
    mount();

    expect(
      await screen.findByText(
        en.preferences.updates.downloadingTitle.replace("{{version}}", "1.3.0"),
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.updates.downloadingNote),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("progressbar", {
        name: en.preferences.updates.downloadProgress,
      }),
    ).toHaveAttribute("aria-valuenow", "50");
    expect(
      screen.getByRole("button", {
        name: en.preferences.updates.backgroundDownloading,
      }),
    ).toBeDisabled();
  });

  it("explains bounded automatic retries without exposing native errors", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
        HttpResponse.json(wire("checking", 2)),
      ),
    );
    mount();

    expect(
      await screen.findByText(
        en.preferences.updates.retryingNote
          .replace("{{attempt}}", "2")
          .replace("{{max}}", "3"),
      ),
    ).toBeInTheDocument();
  });

  it("offers one restart action only after the signed package is ready", async () => {
    let installs = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
        HttpResponse.json(wire("ready")),
      ),
      http.post(`${TAURI_ENDPOINT}/app_update_install_and_restart`, () => {
        installs += 1;
        return HttpResponse.json(true);
      }),
    );
    mount();

    const restart = await screen.findByRole("button", {
      name: en.preferences.updates.restart,
    });
    expect(
      screen.getByText(
        en.preferences.updates.readyTitle.replace("{{version}}", "1.3.0"),
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(en.preferences.updates.readyNote),
    ).toBeInTheDocument();
    await userEvent.click(restart);
    await waitFor(() => expect(installs).toBe(1));
  });

  it("says routed tools go back to direct only while any tool is routed", async () => {
    let routed = true;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
        HttpResponse.json(wire("ready")),
      ),
      http.post(`${TAURI_ENDPOINT}/app_routing_overview`, () =>
        HttpResponse.json(routingOverview(routed)),
      ),
    );
    const first = mount();

    expect(
      await screen.findByText(
        `${en.preferences.updates.readyNote} ${en.preferences.updates.routedNote}`,
      ),
    ).toBeInTheDocument();
    first.unmount();

    routed = false;
    mount();
    expect(
      await screen.findByText(en.preferences.updates.readyNote),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(en.preferences.updates.routedNote, { exact: false }),
    ).toBeNull();
  });

  it("keeps a native status failure visible and lets the user retry", async () => {
    let attempts = 0;
    let downloadPageOpens = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () => {
        attempts += 1;
        return attempts === 1
          ? HttpResponse.text("private update detail", { status: 500 })
          : HttpResponse.json(wire("upToDate"));
      }),
      http.post(`${TAURI_ENDPOINT}/app_update_open_download_page`, () => {
        downloadPageOpens += 1;
        return HttpResponse.json(true);
      }),
    );
    mount();

    const alert = await screen.findByRole("alert", {
      name: en.preferences.updates.error.title,
    });
    expect(alert).toHaveTextContent(en.preferences.updates.error.description);
    expect(alert).not.toHaveTextContent("private update detail");

    await userEvent.click(
      screen.getByRole("button", {
        name: en.preferences.updates.openDownloadPage,
      }),
    );
    await waitFor(() => expect(downloadPageOpens).toBe(1));

    await userEvent.click(
      screen.getByRole("button", { name: en.preferences.updates.retry }),
    );
    await waitFor(() => expect(attempts).toBe(2));
    expect(
      await screen.findByText(en.preferences.updates.upToDate),
    ).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });
});
