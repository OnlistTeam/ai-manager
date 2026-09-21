import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import i18n from "i18next";
import { beforeEach, describe, expect, it, vi } from "vitest";
import en from "@/i18n/locales/en.json";
import { ProviderRuntimeContextPanel } from "@/pages/data/ProviderRuntimeContextPanel";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";
import { server } from "../../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

function mount(toolName: string | null = "Codex CLI") {
  const client = createTestQueryClient();
  render(<ProviderRuntimeContextPanel tool="codex" toolName={toolName} />, {
    wrapper: withQueryClient(client),
  });
}

describe("ProviderRuntimeContextPanel", () => {
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

  it("shows live tool files and storage usage", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () => {
        reads += 1;
        return HttpResponse.json({
          tool: "codex",
          liveConfigPaths: ["~/.codex/config.toml", "~/.codex/auth.json"],
          resources: [
            {
              id: "live-config-0",
              kind: "configuration",
              scope: "global",
              path: "~/.codex/config.toml",
              exists: true,
              action: "browse",
              sizeBytes: 2_048,
              measurementLimited: false,
            },
            {
              id: "global-instructions",
              kind: "instructions",
              scope: "global",
              path: "~/.codex/AGENTS.md",
              exists: false,
              action: "edit",
              sizeBytes: null,
              measurementLimited: false,
            },
            {
              id: "live-config-1",
              kind: "configuration",
              scope: "global",
              path: "~/.codex/auth.json",
              exists: true,
              action: "browse",
              sizeBytes: 512,
              measurementLimited: false,
            },
            {
              id: "session-data-0",
              kind: "sessionData",
              scope: "project",
              path: "~/.codex/sessions",
              exists: true,
              action: "browse",
              sizeBytes: 18_874_368,
              measurementLimited: false,
            },
          ],
          storage: {
            totalBytes: 18_876_928,
            sessionBytes: 18_874_368,
            sessionCount: 24,
            measurementLimited: false,
          },
          effectiveConnection: null,
        });
      }),
    );
    mount();

    // The card's own title is the selected tool's name, so it no longer duplicates the outer "Local context & storage" heading.
    expect(screen.getByText("Codex CLI")).toBeInTheDocument();
    expect(
      screen.queryByText(en.services.runtime.title),
    ).not.toBeInTheDocument();

    expect(await screen.findByText("~/.codex/AGENTS.md")).toBeInTheDocument();
    expect(screen.getByText("~/.codex/sessions")).toBeInTheDocument();
    // The only entry point to the config file is the "Open config file" button on the
    // provider card on the endpoints page (ServicesOpenConfigAction, ADR-0036 decision 3):
    // this panel, moved to the "Local data" page, must not re-list kind === "configuration"
    // rows, or there would be two entry points.
    expect(screen.queryByText("~/.codex/config.toml")).not.toBeInTheDocument();
    expect(screen.queryByText("~/.codex/auth.json")).not.toBeInTheDocument();
    expect(screen.getAllByText("18 MB").length).toBeGreaterThan(0);
    expect(
      screen.getByText(en.services.runtime.notCreated),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: en.services.runtime.action.create }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/override/i)).not.toBeInTheDocument();

    await userEvent.click(
      screen.getByRole("button", { name: en.services.runtime.refresh }),
    );
    await waitFor(() => expect(reads).toBe(2));
  });

  it("opens only a backend-owned resource id and reports the native outcome", async () => {
    const seen: unknown[] = [];
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () => {
        reads += 1;
        return HttpResponse.json({
          tool: "codex",
          liveConfigPaths: ["~/.codex/config.toml"],
          resources: [
            {
              id: "global-instructions",
              kind: "instructions",
              scope: "global",
              path: "~/.codex/AGENTS.md",
              exists: false,
              action: "edit",
              sizeBytes: null,
              measurementLimited: false,
            },
          ],
          storage: {
            totalBytes: 0,
            sessionBytes: 0,
            sessionCount: 0,
            measurementLimited: false,
          },
          effectiveConnection: null,
        });
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_runtime_resource_open`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("editorOpened");
        },
      ),
    );
    mount();

    await userEvent.click(
      await screen.findByRole("button", {
        name: en.services.runtime.action.create,
      }),
    );

    expect(seen).toEqual([{ tool: "codex", resource: "global-instructions" }]);
    await waitFor(() => expect(reads).toBe(2));
  });

  it("falls back to the generic title when no tool name was supplied", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
          tool: "codex",
          liveConfigPaths: [],
          resources: [],
          storage: {
            totalBytes: 0,
            sessionBytes: 0,
            sessionCount: 0,
            measurementLimited: false,
          },
          effectiveConnection: null,
        }),
      ),
    );
    mount(null);

    expect(
      await screen.findByText(en.services.runtime.title),
    ).toBeInTheDocument();
  });

  it("embeds a flat file list with refresh beside storage and preserves managed-prompt handoff", async () => {
    const onManage = vi.fn();
    const opened: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () =>
        HttpResponse.json({
          tool: "codex",
          liveConfigPaths: ["~/.codex/config.toml"],
          resources: [
            {
              id: "live-config-0",
              kind: "configuration",
              scope: "global",
              path: "~/.codex/config.toml",
              exists: true,
              action: "edit",
              sizeBytes: 1024,
              measurementLimited: false,
            },
            {
              id: "global-instructions",
              kind: "instructions",
              scope: "global",
              path: "~/.codex/AGENTS.md",
              exists: true,
              action: "edit",
              sizeBytes: 2048,
              measurementLimited: false,
            },
          ],
          storage: {
            totalBytes: 3072,
            sessionBytes: 0,
            sessionCount: 0,
            measurementLimited: true,
          },
          effectiveConnection: null,
        }),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_runtime_resource_open`,
        async ({ request }) => {
          opened.push(await request.json());
          return HttpResponse.json("editorOpened");
        },
      ),
    );
    const { container } = render(
      <ProviderRuntimeContextPanel
        tool="codex"
        toolName="Codex CLI"
        embedded
        includeConfiguration
        onManageInstructions={onManage}
      />,
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    const path = await screen.findByText("~/.codex/config.toml");
    expect(screen.queryByText("Codex CLI")).not.toBeInTheDocument();
    expect(container.querySelector(".provider-runtime-panel")).toBeNull();
    expect(path).not.toHaveClass("border");
    expect(path.closest("li")).not.toHaveClass("border");
    expect(screen.getByText("≥ 3 KB")).toBeInTheDocument();
    expect(
      screen.getByText(en.services.runtime.storage.limited),
    ).toBeInTheDocument();
    const refresh = screen.getByRole("button", {
      name: en.services.runtime.refresh,
    });
    expect(refresh.parentElement).toContainElement(
      screen.getByText(en.services.runtime.storage.total),
    );
    await userEvent.click(
      screen.getByRole("button", { name: en.services.runtime.action.manage }),
    );
    expect(onManage).toHaveBeenCalledOnce();
    expect(opened).toEqual([]);
    await userEvent.click(
      screen.getByRole("button", { name: en.services.runtime.action.edit }),
    );
    await waitFor(() =>
      expect(opened).toEqual([{ tool: "codex", resource: "live-config-0" }]),
    );
  });

  it("shows refresh failure without removing the last successful context", async () => {
    let reads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_runtime_context`, () => {
        reads += 1;
        return reads === 1
          ? HttpResponse.json({
              tool: "codex",
              liveConfigPaths: [],
              resources: [],
              storage: {
                totalBytes: 1024,
                sessionBytes: 0,
                sessionCount: 0,
                measurementLimited: false,
              },
              effectiveConnection: null,
            })
          : HttpResponse.json(
              {
                code: "UPSTREAM_ERROR",
                messageKey: "error.provider.runtimeResourceNotFound",
                technicalMessage: "private path",
                remediation: null,
                contextId: null,
              },
              { status: 500 },
            );
      }),
    );
    mount();
    await userEvent.click(
      await screen.findByRole("button", { name: en.services.runtime.refresh }),
    );
    expect(
      await screen.findByRole("button", { name: en.ds.action.retry }),
    ).toBeInTheDocument();
    expect(screen.getByText("1 KB")).toBeInTheDocument();
    expect(document.body).not.toHaveTextContent("private path");
  });
});
