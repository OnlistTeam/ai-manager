import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { server } from "../msw/server";
import { native } from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";

/**
 * The model probe is the one provider path that carries readable text back to
 * the renderer (ADR-0041). These tests pin what it may and may not carry.
 */
describe("native.providers model probe", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("lists models without sending an address or a key", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_models_list`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            protocol: "openAi",
            models: [
              { id: "gpt-5.2", kind: "text" },
              { id: "gpt-image-2", kind: "image" },
            ],
            truncated: false,
            rejection: null,
          });
        },
      ),
    );

    const catalog = await native.providers.listModels("codex", "relay");
    expect(seen).toEqual([{ tool: "codex", provider: "relay" }]);
    expect(catalog.models).toHaveLength(2);
    expect(catalog.models[1]?.kind).toBe("image");
  });

  it("accepts a refused catalogue as a successful answer", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json({
          protocol: "openAi",
          models: [],
          truncated: false,
          rejection: { status: 401, detail: "invalid api key" },
        }),
      ),
    );

    const catalog = await native.providers.listModels("codex", "relay");
    expect(catalog.rejection).toEqual({
      status: 401,
      detail: "invalid api key",
    });
  });

  it("rejects a catalogue that both refuses and lists models", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json({
          protocol: "openAi",
          models: [{ id: "gpt-5.2", kind: "text" }],
          truncated: false,
          rejection: { status: 401, detail: "invalid api key" },
        }),
      ),
    );

    await expect(
      native.providers.listModels("codex", "relay"),
    ).rejects.toThrow();
  });

  it("rejects a catalogue carrying an unknown field", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_models_list`, () =>
        HttpResponse.json({
          protocol: "openAi",
          models: [],
          truncated: false,
          rejection: null,
          baseUrl: "https://relay.example.com",
        }),
      ),
    );

    await expect(
      native.providers.listModels("codex", "relay"),
    ).rejects.toThrow();
  });

  it("sends the probe request and reads a text reply", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_provider_model_probe`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            model: "gpt-5.2",
            latencyMs: 1_800,
            httpStatus: 200,
            suggestedBaseUrl: null,
            reply: { kind: "text", text: "pong" },
          });
        },
      ),
    );

    const outcome = await native.providers.probeModel("codex", "relay", {
      model: "gpt-5.2",
      kind: "text",
      prompt: "ping",
    });

    expect(seen).toEqual([
      {
        tool: "codex",
        provider: "relay",
        request: { model: "gpt-5.2", kind: "text", prompt: "ping" },
      },
    ]);
    expect(outcome.reply).toEqual({ kind: "text", text: "pong" });
  });

  it("reads an image reply as inline data", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_model_probe`, () =>
        HttpResponse.json({
          model: "gpt-image-2",
          latencyMs: 42_000,
          httpStatus: 200,
          suggestedBaseUrl: null,
          reply: {
            kind: "image",
            mime: "image/png",
            base64: "iVBORw0KGgoAAAA",
          },
        }),
      ),
    );

    const outcome = await native.providers.probeModel("codex", "relay", {
      model: "gpt-image-2",
      kind: "image",
      prompt: "a red circle",
    });

    expect(outcome.reply).toEqual({
      kind: "image",
      mime: "image/png",
      base64: "iVBORw0KGgoAAAA",
    });
  });

  it("refuses to send an empty or oversized prompt", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_model_probe`, () => {
        calls += 1;
        return HttpResponse.json({});
      }),
    );

    // The guard runs before the request is built, so it throws synchronously.
    expect(() =>
      native.providers.probeModel("codex", "relay", {
        model: "gpt-5.2",
        kind: "text",
        prompt: "   ",
      }),
    ).toThrow();

    expect(() =>
      native.providers.probeModel("codex", "relay", {
        model: "gpt-5.2",
        kind: "text",
        prompt: "x".repeat(201),
      }),
    ).toThrow();

    expect(calls).toBe(0);
  });

  it("rejects a reply whose kind is not one this app can render", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_provider_model_probe`, () =>
        HttpResponse.json({
          model: "gpt-5.2",
          latencyMs: 10,
          httpStatus: 200,
          suggestedBaseUrl: null,
          reply: { kind: "video", url: "https://cdn.example.test/a.mp4" },
        }),
      ),
    );

    await expect(
      native.providers.probeModel("codex", "relay", {
        model: "gpt-5.2",
        kind: "text",
        prompt: "ping",
      }),
    ).rejects.toThrow();
  });
});
