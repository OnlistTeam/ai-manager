import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import type { ProviderRuntimeResource } from "@/entities/provider";
import { ServicesOpenConfigAction } from "@/pages/services/ServicesOpenConfigAction";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

function resource(action: "edit" | "browse"): ProviderRuntimeResource {
  return {
    id: "live-config-0",
    kind: "configuration",
    scope: "global",
    path: "~/.codex/config.toml",
    exists: action === "edit",
    action,
    sizeBytes: 2_048,
    measurementLimited: false,
  };
}

function mount(value: ProviderRuntimeResource | undefined) {
  render(<ServicesOpenConfigAction tool="codex" resource={value} />, {
    wrapper: withQueryClient(createTestQueryClient()),
  });
}

describe("ServicesOpenConfigAction", () => {
  beforeEach(async () => {
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("opens the file itself once that file exists", async () => {
    const opened = vi.fn();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_resource_open`, () => {
        opened();
        return HttpResponse.json("editorOpened");
      }),
    );
    mount(resource("edit"));

    await userEvent.click(
      screen.getByRole("button", { name: en.services.runtime.openConfigFile }),
    );
    await waitFor(() => expect(opened).toHaveBeenCalledTimes(1));
  });

  it("promises only the folder while the file is still missing", () => {
    mount(resource("browse"));

    // The button must not say "Open config file": that file doesn't exist yet, so clicking it can only land on the folder.
    expect(
      screen.getByRole("button", {
        name: en.services.runtime.openConfigFolder,
      }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: en.services.runtime.openConfigFile,
      }),
    ).not.toBeInTheDocument();
  });

  it("renders nothing when the tool reports no configuration file", () => {
    mount(undefined);
    expect(
      screen.queryByRole("button", {
        name: en.services.runtime.openConfigFolder,
      }),
    ).not.toBeInTheDocument();
  });
});
