import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const overview = {
  files: [
    {
      id: "agents",
      filename: "AGENTS.md",
      status: "ready",
      sizeBytes: 12,
      modifiedAt: 1_777_000_000,
    },
  ],
  existingFiles: 1,
  dailyMemoryCount: 2,
  dailyMemoryBytes: 24,
  totalBytes: 36,
  limited: false,
};

describe("native.openClawWorkspace", () => {
  it("reads a path-free workspace overview", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_overview`, () =>
        HttpResponse.json(overview),
      ),
    );
    await expect(native.openClawWorkspace.overview()).resolves.toEqual(
      overview,
    );
  });

  it("selects documents by a fixed id and never sends a filename or path", async () => {
    let payload: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_workspace_document`,
        async ({ request }) => {
          payload = await request.json();
          return HttpResponse.json({
            id: "agents",
            filename: "AGENTS.md",
            exists: true,
            content: "# Agent rules",
            sizeBytes: 13,
            modifiedAt: null,
          });
        },
      ),
    );
    await expect(
      native.openClawWorkspace.document("agents"),
    ).resolves.toMatchObject({ content: "# Agent rules" });
    expect(payload).toEqual({ file: "agents" });
  });

  it("saves daily memory by date without accepting a raw path", async () => {
    let payload: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_daily_memory_save`,
        async ({ request }) => {
          payload = await request.json();
          return HttpResponse.json({ backupCreated: true });
        },
      ),
    );
    await expect(
      native.openClawWorkspace.saveMemory("2026-08-26", "local note"),
    ).resolves.toEqual({ backupCreated: true });
    expect(payload).toEqual({ date: "2026-08-26", content: "local note" });
    expect(JSON.stringify(payload)).not.toMatch(/path|filename|directory/iu);
  });

  it("opens only fixed native directory targets", async () => {
    let payload: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_openclaw_workspace_open_directory`,
        async ({ request }) => {
          payload = await request.json();
          return HttpResponse.json(null);
        },
      ),
    );
    await expect(
      native.openClawWorkspace.openDirectory("daily-memory"),
    ).resolves.toBeUndefined();
    expect(payload).toEqual({ directory: "daily-memory" });
  });

  it.each([
    ["overview path", { ...overview, path: "/Users/private/.openclaw" }],
    [
      "document path",
      {
        id: "agents",
        filename: "AGENTS.md",
        exists: true,
        content: "safe",
        sizeBytes: 4,
        modifiedAt: null,
        sourcePath: "/Users/private/.openclaw/workspace/AGENTS.md",
      },
    ],
  ])("rejects a response containing %s", async (label, unsafe) => {
    const endpoint = label === "overview path" ? "overview" : "document";
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_openclaw_workspace_${endpoint}`, () =>
        HttpResponse.json(unsafe),
      ),
    );
    const request =
      endpoint === "overview"
        ? native.openClawWorkspace.overview()
        : native.openClawWorkspace.document("agents");
    await expect(request).rejects.toBeInstanceOf(NativeError);
  });
});
