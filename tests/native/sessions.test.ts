import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { native, NativeError } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const reference = "a".repeat(64);
const summary = {
  reference,
  tool: "codex",
  title: "Fix release tests",
  preview: "Inspect the native boundary",
  projectName: "ai-manager",
  createdAt: 1_777_000_000,
  lastActiveAt: 1_777_000_100,
  resumable: true,
};

describe("native.sessions", () => {
  let payloads: unknown[];

  beforeEach(() => {
    payloads = [];
  });

  it("lists sessions through nullable search and tool filters", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, async ({ request }) => {
        payloads.push(await request.json());
        return HttpResponse.json({
          items: [summary],
          totalCount: 1,
          limited: false,
        });
      }),
    );

    await expect(native.sessions.list("", null)).resolves.toMatchObject({
      items: [{ tool: "codex", title: "Fix release tests" }],
    });
    await native.sessions.list(" release ", "codex");
    expect(payloads).toEqual([
      { query: null, tool: null },
      { query: "release", tool: "codex" },
    ]);
  });

  it("loads a bounded thread only by opaque reference", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_thread`, async ({ request }) => {
        payloads.push(await request.json());
        return HttpResponse.json({
          reference,
          messages: [
            {
              role: "assistant",
              content: "Done",
              timestamp: 1_777_000_200,
              truncated: false,
            },
          ],
          totalCount: 1,
          limited: false,
          workingDirectory: "/work/project",
          resumeCommand: "cd /work/project && codex resume session-42",
        });
      }),
    );

    await expect(native.sessions.thread(reference)).resolves.toMatchObject({
      messages: [{ role: "assistant", content: "Done" }],
      workingDirectory: "/work/project",
      resumeCommand: "cd /work/project && codex resume session-42",
    });
    expect(payloads).toEqual([{ reference }]);
  });

  it("resumes without accepting a command or path from the renderer", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_session_resume`, async ({ request }) => {
        payloads.push(await request.json());
        return HttpResponse.json(null);
      }),
    );

    await expect(native.sessions.resume(reference)).resolves.toBeUndefined();
    expect(payloads).toEqual([{ reference }]);
  });

  it.each([
    [
      "raw source path",
      { ...summary, sourcePath: "/Users/alice/.codex/session" },
    ],
    ["raw session id", { ...summary, sessionId: "private-id" }],
    ["raw command", { ...summary, resumeCommand: "codex resume private-id" }],
  ])("rejects a list response containing %s", async (_label, unsafe) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_sessions_list`, () =>
        HttpResponse.json({ items: [unsafe], totalCount: 1, limited: false }),
      ),
    );

    await expect(native.sessions.list("", null)).rejects.toBeInstanceOf(
      NativeError,
    );
  });
});
