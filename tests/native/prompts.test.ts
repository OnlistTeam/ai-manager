import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { native, NativeError, type PromptDraft } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";

const draft: PromptDraft = {
  name: "Team rules",
  description: "Shared coding rules.",
  content: "# Rules\n\nRun tests.",
};

const detail = {
  id: "prompt-1",
  tool: "claude-code",
  name: "Team rules",
  description: "Shared coding rules.",
  content: "# Rules\n\nRun tests.",
  enabled: false,
};

const inventory = [
  {
    kind: "prompt",
    id: "prompt-1",
    scope: { kind: "tool", id: "claude-code" },
    name: "Team rules",
    description: "Shared coding rules.",
    management: "managed",
    enabled: false,
    canDisable: false,
  },
];

describe("native.prompts", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("loads content only through the explicit detail command", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_get`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json(detail);
      }),
    );
    await expect(
      native.prompts.get("claude-code", "prompt-1"),
    ).resolves.toEqual(detail);
    expect(seen).toEqual([{ tool: "claude-code", prompt: "prompt-1" }]);
  });

  it("creates and edits with one inbound-only draft", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json(inventory);
      }),
    );
    await native.prompts.save("claude-code", null, draft);
    await native.prompts.save("claude-code", "prompt-1", draft);
    expect(seen).toEqual([
      { tool: "claude-code", prompt: null, draft },
      { tool: "claude-code", prompt: "prompt-1", draft },
    ]);
  });

  it("removes and imports without sending paths or content", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_remove`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json([]);
      }),
      http.post(
        `${TAURI_ENDPOINT}/app_prompt_import_current`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json(inventory);
        },
      ),
    );
    await native.prompts.remove("claude-code", "prompt-1");
    await native.prompts.importCurrent("claude-code");
    expect(seen).toEqual([
      { tool: "claude-code", prompt: "prompt-1" },
      { tool: "claude-code" },
    ]);
  });

  it("fails closed if detail unexpectedly contains a filesystem path", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_get`, () =>
        HttpResponse.json({
          ...detail,
          path: "/Users/alice/.claude/CLAUDE.md",
        }),
      ),
    );
    await expect(
      native.prompts.get("claude-code", "prompt-1"),
    ).rejects.toBeInstanceOf(NativeError);
  });

  it("rejects an empty draft before invoking native", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_prompt_save`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json(inventory);
      }),
    );
    expect(() =>
      native.prompts.save("claude-code", null, { ...draft, content: "  " }),
    ).toThrow();
    expect(seen).toEqual([]);
  });
});
