import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it } from "vitest";
import { GlobalUpdateAction } from "@/features/updater/GlobalUpdateAction";
import en from "@/i18n/locales/en.json";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function wire(phase: "unconfigured" | "downloading" | "ready") {
  return {
    currentVersion: "1.2.3",
    availableVersion: phase === "unconfigured" ? null : "1.3.0",
    channelReady: phase !== "unconfigured",
    phase,
    downloadedBytes: phase === "downloading" ? 51 : phase === "ready" ? 100 : 0,
    totalBytes: phase === "unconfigured" ? null : 100,
    attempt: phase === "unconfigured" ? 0 : 1,
    maxAttempts: phase === "unconfigured" ? 0 : 3,
  };
}

function mount() {
  return render(<GlobalUpdateAction />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("GlobalUpdateAction", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { error: en.error, preferences: en.preferences },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("keeps the persistent chrome quiet while the channel is unconfigured", async () => {
    let requests = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () => {
        requests += 1;
        return HttpResponse.json(wire("unconfigured"));
      }),
    );

    const { container } = mount();
    await waitFor(() => expect(requests).toBe(1));
    expect(container).toBeEmptyDOMElement();
  });

  it("shows compact background progress without taking over the window", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_update_start`, () =>
        HttpResponse.json(wire("downloading")),
      ),
    );
    mount();

    const status = await screen.findByRole("status", {
      name: en.preferences.updates.backgroundDownloading,
    });
    expect(status).toHaveTextContent("51%");
    expect(
      screen.queryByRole("button", {
        name: en.preferences.updates.restart,
      }),
    ).toBeNull();
  });

  it("offers restart only after the signed update is ready", async () => {
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

    await userEvent.click(
      await screen.findByRole("button", {
        name: en.preferences.updates.restart,
      }),
    );
    await waitFor(() => expect(installs).toBe(1));
  });
});
