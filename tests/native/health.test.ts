import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const SNAPSHOT = {
  providers: [
    {
      tool: "claude-code",
      configured: true,
      configuredCount: 1,
      checkTargets: [{ providerId: "relay", name: "Relay" }],
    },
  ],
  configs: [{ tool: "claude-code", status: "readable" }],
  mcp: { total: 3, enabled: 2 },
};

describe("native.health", () => {
  it("sends only the installed product tool ids and parses the snapshot", async () => {
    let received: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_health_snapshot`,
        async ({ request }) => {
          received = await request.json();
          return HttpResponse.json(SNAPSHOT);
        },
      ),
    );
    await expect(native.health.snapshot(["claude-code"])).resolves.toEqual(
      SNAPSHOT,
    );
    expect(received).toEqual({ tools: ["claude-code"] });
  });

  it("accepts a live configuration that does not exist yet", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          ...SNAPSHOT,
          configs: [{ tool: "gemini-cli", status: "missing" }],
        }),
      ),
    );
    await expect(native.health.snapshot(["gemini-cli"])).resolves.toMatchObject(
      {
        configs: [{ tool: "gemini-cli", status: "missing" }],
      },
    );
  });

  it("rejects contradictory counts and any diagnostic payload", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_health_snapshot`, () =>
        HttpResponse.json({
          ...SNAPSHOT,
          providers: [
            {
              ...SNAPSHOT.providers[0],
              configured: false,
              settingsConfig: { apiKey: "secret" },
            },
          ],
          mcp: { total: 1, enabled: 2 },
        }),
      ),
    );
    await expect(native.health.snapshot(["claude-code"])).rejects.toMatchObject(
      {
        messageKey: "error.native.responseSchemaMismatch",
      },
    );
  });
});
