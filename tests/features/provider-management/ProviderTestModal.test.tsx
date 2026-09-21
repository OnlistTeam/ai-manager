import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import i18n from "i18next";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";

import en from "@/i18n/locales/en.json";
import {
  ProviderTestModal,
  type ProbeSubject,
} from "@/features/provider-management";
import type { Provider } from "@/entities/provider";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const provider: Provider = {
  additive: false,
  id: "relay",
  tool: "codex",
  name: "My Relay",
  kind: "custom",
  active: true,
  baseUrl: "https://relay.example.test",
  apiKey: "key-placeholder",
  websiteUrl: null,
  testable: true,
  canRemove: true,
};

function editProfile(overrides: Record<string, unknown> = {}) {
  return {
    providerId: "relay",
    baseUrl: "https://relay.example.test",
    endpointCandidates: [],
    endpointAutoSelect: false,
    models: ["gpt-5.2"],
    headerNames: [],
    capabilities: {
      canEditBaseUrl: true,
      canEditEndpoints: false,
      canEditModels: true,
      canEditHeaders: false,
      supportsMultipleModels: false,
    },
    ...overrides,
  };
}

function catalog(overrides: Record<string, unknown> = {}) {
  return {
    protocol: "openAi",
    models: [
      { id: "gpt-5.2", kind: "text" },
      { id: "gpt-image-2", kind: "image" },
    ],
    truncated: false,
    rejection: null,
    ...overrides,
  };
}

function mount(subject: ProbeSubject | null = { kind: "provider", provider }) {
  const client = createTestQueryClient();
  return {
    client,
    ...render(<ProviderTestModal subject={subject} onOpenChange={vi.fn()} />, {
      wrapper: withQueryClient(client),
    }),
  };
}

describe("ProviderTestModal", () => {
  beforeEach(async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_test`, () =>
        HttpResponse.json({
          providerId: "relay",
          reachability: "operational",
          responseTimeMs: 240,
          httpStatus: 200,
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_edit_profile`, () =>
        HttpResponse.json(editProfile()),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json(catalog()),
      ),
    );
    i18n.addResourceBundle(
      "en",
      "translation",
      { ds: en.ds, error: en.error, services: en.services },
      true,
      true,
    );
    await i18n.changeLanguage("en");
  });

  it("checks the address, lists models and preselects the configured one", async () => {
    mount();
    const dialog = await screen.findByRole("dialog");

    expect(
      await within(dialog).findByText(en.services.test.operational),
    ).toBeInTheDocument();
    await waitFor(() =>
      expect(
        within(dialog).getByLabelText(en.services.probe.model),
      ).toHaveValue("gpt-5.2"),
    );
  });

  it("sends the selected model and renders the reply", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_model_probe`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            model: "gpt-5.2",
            latencyMs: 1_800,
            httpStatus: 200,
            reply: { kind: "text", text: "pong" },
          });
        },
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText(en.services.probe.model);

    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.probe.send }),
    );

    expect(await within(dialog).findByText("pong")).toBeInTheDocument();
    expect(seen).toEqual([
      {
        tool: "codex",
        provider: "relay",
        request: { model: "gpt-5.2", kind: "text", prompt: "hi" },
      },
    ]);
  });

  it("switches to an image request when an image model is chosen", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_model_probe`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            model: "gpt-image-2",
            latencyMs: 40_000,
            httpStatus: 200,
            reply: {
              kind: "image",
              mime: "image/png",
              base64: "iVBORw0KGgoAAAA",
            },
          });
        },
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText(en.services.probe.model);

    await userEvent.selectOptions(
      within(dialog).getByLabelText(en.services.probe.model),
      "gpt-image-2",
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.probe.send }),
    );

    expect(seen).toEqual([
      {
        tool: "codex",
        provider: "relay",
        request: { model: "gpt-image-2", kind: "image", prompt: "hi" },
      },
    ]);
    const image = await within(dialog).findByRole("img");
    expect(image).toHaveAttribute(
      "src",
      "data:image/png;base64,iVBORw0KGgoAAAA",
    );
  });

  it("falls back to a typed model name when the service publishes no catalogue", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json(catalog({ models: [] })),
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");

    const field = await within(dialog).findByLabelText(
      en.services.probe.modelManual,
    );
    // The saved model seeds the box, so the test still works without a list.
    await waitFor(() => expect(field).toHaveValue("gpt-5.2"));
    expect(
      within(dialog).getByText(en.services.probe.modelManualHint),
    ).toBeInTheDocument();
    // The protocol still arrives, so image generation stays offered.
    expect(
      within(dialog).getByRole("tab", { name: en.services.probe.kindImage }),
    ).toBeInTheDocument();
  });

  it("reports a catalogue request that could not run at all", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json(
          {
            code: "NETWORK_ERROR",
            messageKey: "error.provider.modelProbeTimeout",
            technicalMessage: "model probe timed out",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");

    expect(
      await within(dialog).findByText(en.error.provider.modelProbeTimeout),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByLabelText(en.services.probe.modelManual),
    ).toBeInTheDocument();
  });

  it("reports a refused catalogue without blocking the test", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json(
          catalog({
            models: [],
            rejection: { status: 401, detail: "invalid api key" },
          }),
        ),
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");

    expect(
      await within(dialog).findByText(
        en.services.probe.modelRefused.replace("{{status}}", "401"),
      ),
    ).toBeInTheDocument();
    expect(
      within(dialog).getByLabelText(en.services.probe.modelManual),
    ).toBeInTheDocument();
  });

  it("shows the service's own refusal as the result rather than an error card", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_model_probe`, () =>
        HttpResponse.json({
          model: "gpt-5.2",
          latencyMs: 300,
          httpStatus: 401,
          reply: { kind: "rejected", detail: "invalid api key" },
        }),
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText(en.services.probe.model);

    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.probe.send }),
    );

    expect(
      await within(dialog).findByText(en.services.probe.replyRefused),
    ).toBeInTheDocument();
    expect(within(dialog).getByText("invalid api key")).toBeInTheDocument();
  });

  it("writes only the model back when the tested model is adopted", async () => {
    const saved: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_edit_profile`, () =>
        HttpResponse.json(editProfile({ models: ["gpt-4o"] })),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_model_probe`, () =>
        HttpResponse.json({
          model: "gpt-5.2",
          latencyMs: 900,
          httpStatus: 200,
          reply: { kind: "text", text: "pong" },
        }),
      ),
      http.post(`${TAURI_ENDPOINT}/app_provider_save`, async ({ request }) => {
        saved.push(await request.json());
        return HttpResponse.json([provider]);
      }),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText(en.services.probe.model);

    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.probe.send }),
    );
    await within(dialog).findByText("pong");
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.probe.adopt }),
    );

    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({
      tool: "codex",
      provider: "relay",
      draft: { name: "My Relay", apiKey: null, models: ["gpt-5.2"] },
    });
    expect(
      await within(dialog).findByText(en.services.probe.adopted),
    ).toBeInTheDocument();
  });

  it("never sends the probe on its own", async () => {
    let probes = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_model_probe`, () => {
        probes += 1;
        return HttpResponse.json({
          model: "gpt-5.2",
          latencyMs: 1,
          httpStatus: 200,
          reply: { kind: "empty" },
        });
      }),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText(en.services.probe.model);
    await within(dialog).findByText(en.services.test.operational);

    expect(probes).toBe(0);
  });

  /**
   * A relay set up for Claude Code speaks the Anthropic dialect for chat and
   * serves `openai/gpt-image-…` alongside it. The switch used to appear only
   * for OpenAI-dialect services, so on this endpoint it was hidden while the
   * name heuristic still selected Image — one dead end with no way back.
   */
  it("offers the image switch whenever the catalogue lists an image model", async () => {
    const probed: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json(
          catalog({
            protocol: "anthropic",
            models: [
              { id: "claude-opus-5", kind: "text" },
              { id: "openai/gpt-image-2.5-sunburst", kind: "image" },
            ],
          }),
        ),
      ),
      http.post(
        `${TAURI_ENDPOINT}/app_provider_model_probe`,
        async ({ request }) => {
          probed.push(await request.json());
          return HttpResponse.json({
            model: "openai/gpt-image-2.5-sunburst",
            latencyMs: 30_000,
            httpStatus: 200,
            reply: { kind: "image", mime: "image/png", base64: "aGk=" },
          });
        },
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    const picker = await within(dialog).findByLabelText(
      en.services.probe.model,
    );

    await userEvent.selectOptions(picker, "openai/gpt-image-2.5-sunburst");
    const imageTab = within(dialog).getByRole("tab", {
      name: en.services.probe.kindImage,
    });
    expect(imageTab).toHaveAttribute("aria-selected", "true");

    // And the way back is there, which is the part that was missing.
    await userEvent.click(
      within(dialog).getByRole("tab", { name: en.services.probe.kindText }),
    );
    await userEvent.click(
      within(dialog).getByRole("button", { name: en.services.probe.send }),
    );
    expect(probed).toEqual([
      {
        tool: "codex",
        provider: "relay",
        request: {
          model: "openai/gpt-image-2.5-sunburst",
          kind: "text",
          prompt: "hi",
        },
      },
    ]);
  });

  /**
   * With no evidence of image generation the switch stays away — offering it on
   * `api.anthropic.com` would promise something Claude cannot do.
   */
  it("hides the image switch when nothing on offer generates images", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json(
          catalog({
            protocol: "anthropic",
            models: [{ id: "claude-opus-5", kind: "text" }],
          }),
        ),
      ),
    );
    mount();
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByLabelText(en.services.probe.model);
    expect(
      within(dialog).queryByRole("tab", { name: en.services.probe.kindImage }),
    ).toBeNull();
  });

  it("renders nothing until a service is chosen", () => {
    mount(null);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  /**
   * A connection a shell profile exports is the one the tool will really use,
   * so it gets the same dialog. It has no saved record, which means no address
   * check to run first and nowhere to write a tested model back to.
   */
  describe("the connection in force with no saved record", () => {
    it("tests it without ever naming a service", async () => {
      const listed: unknown[] = [];
      const probed: unknown[] = [];
      server.use(
        http.post(
          `${TAURI_ENDPOINT}/app_provider_effective_models_list`,
          async ({ request }) => {
            listed.push(await request.json());
            return HttpResponse.json(catalog());
          },
        ),
        http.post(
          `${TAURI_ENDPOINT}/app_provider_effective_model_probe`,
          async ({ request }) => {
            probed.push(await request.json());
            return HttpResponse.json({
              model: "gpt-5.2",
              latencyMs: 700,
              httpStatus: 200,
              reply: { kind: "text", text: "pong" },
            });
          },
        ),
      );
      mount({ kind: "effective", tool: "codex", name: "Codex" });
      const dialog = await screen.findByRole("dialog");
      await within(dialog).findByLabelText(en.services.probe.model);

      await userEvent.click(
        within(dialog).getByRole("button", { name: en.services.probe.send }),
      );

      expect(await within(dialog).findByText("pong")).toBeInTheDocument();
      // No id in either request: the backend resolves the connection itself.
      expect(listed).toEqual([{ tool: "codex" }]);
      expect(probed).toEqual([
        {
          tool: "codex",
          request: { model: "gpt-5.2", kind: "text", prompt: "hi" },
        },
      ]);
    });

    it("offers no address check and no way to adopt the model", async () => {
      server.use(
        http.post(`${TAURI_ENDPOINT}/app_provider_effective_models_list`, () =>
          HttpResponse.json(catalog()),
        ),
        http.post(`${TAURI_ENDPOINT}/app_provider_effective_model_probe`, () =>
          HttpResponse.json({
            model: "gpt-5.2",
            latencyMs: 700,
            httpStatus: 200,
            reply: { kind: "text", text: "pong" },
          }),
        ),
      );
      mount({ kind: "effective", tool: "codex", name: "Codex" });
      const dialog = await screen.findByRole("dialog");

      expect(
        await within(dialog).findByText(en.services.probe.effectiveNote),
      ).toBeInTheDocument();
      expect(
        within(dialog).queryByText(en.services.test.operational),
      ).not.toBeInTheDocument();

      await userEvent.click(
        within(dialog).getByRole("button", { name: en.services.probe.send }),
      );
      await within(dialog).findByText("pong");
      expect(
        within(dialog).queryByRole("button", {
          name: en.services.probe.adopt,
        }),
      ).not.toBeInTheDocument();
    });
  });
});
