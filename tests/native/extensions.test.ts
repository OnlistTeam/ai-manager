import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { server } from "../msw/server";
import { native, NativeError } from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";
const SCOPE = { kind: "tool", id: "claude-code" } as const;

const wire = {
  kind: "mcp",
  id: "filesystem",
  scope: SCOPE,
  name: "Filesystem",
  description: "Reads and writes files you pick.",
  management: "managed",
  enabled: true,
  canDisable: true,
};

describe("native.extensions", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("lists one kind of extension for one tool", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_extensions_list`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([wire]);
        },
      ),
    );
    const extensions = await native.extensions.list(SCOPE, "mcp");
    expect(seen).toEqual([{ scope: SCOPE, kind: "mcp" }]);
    expect(extensions[0]?.name).toBe("Filesystem");
    expect(extensions[0]?.canDisable).toBe(true);
  });

  it("reads the safe local inventory without sending a scope", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_extensions_local_inventory`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json({
            items: [
              {
                ...wire,
                management: "detected",
                server: { command: "npx", env: { TOKEN: "secret" } },
                path: "/Users/person/.codex/config.toml",
              },
            ],
            scopes: [{ tool: "claude-code", kind: "mcp", status: "ready" }],
            truncated: false,
          });
        },
      ),
    );

    const inventory = await native.extensions.localInventory();
    expect(seen).toEqual([{}]);
    expect(inventory.items[0]?.name).toBe("Filesystem");
    expect(inventory.items[0]).not.toHaveProperty("server");
    expect(JSON.stringify(inventory)).not.toContain("secret");
    expect(JSON.stringify(inventory)).not.toContain("/Users/person");
  });

  it.each([
    {
      label: "managed item",
      payload: {
        items: [wire],
        scopes: [{ tool: "claude-code", kind: "mcp", status: "ready" }],
        truncated: false,
      },
    },
    {
      label: "duplicate scope",
      payload: {
        items: [],
        scopes: [
          { tool: "claude-code", kind: "mcp", status: "ready" },
          { tool: "claude-code", kind: "mcp", status: "ready" },
        ],
        truncated: false,
      },
    },
    {
      label: "item in unavailable scope",
      payload: {
        items: [{ ...wire, management: "detected" }],
        scopes: [{ tool: "claude-code", kind: "mcp", status: "unavailable" }],
        truncated: false,
      },
    },
  ])("rejects a local inventory with $label", async ({ payload }) => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () =>
        HttpResponse.json(payload),
      ),
    );
    await expect(native.extensions.localInventory()).rejects.toBeInstanceOf(
      NativeError,
    );
  });

  it("switches one on and gets the refreshed list back", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_extension_set_enabled`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([{ ...wire, enabled: false }]);
        },
      ),
    );
    const refreshed = await native.extensions.setEnabled(
      SCOPE,
      "mcp",
      "filesystem",
      false,
    );
    expect(seen).toEqual([
      {
        scope: SCOPE,
        kind: "mcp",
        extension: "filesystem",
        enabled: false,
      },
    ]);
    expect(refreshed[0]?.enabled).toBe(false);
  });

  it("adopts every detected item in one tool scope without sending a payload", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_extensions_adopt_detected`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([wire]);
        },
      ),
    );

    const refreshed = await native.extensions.adoptDetected(SCOPE, "mcp");
    expect(seen).toEqual([{ scope: SCOPE, kind: "mcp" }]);
    expect(refreshed).toEqual([wire]);
  });

  it("opens one detected Skill using only its stable id and a fixed action", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_detected_skill_resource_open`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json("editorOpened");
        },
      ),
    );

    await expect(
      native.extensions.openDetectedSkillResource(
        SCOPE,
        "mobile-app-release",
        "edit",
      ),
    ).resolves.toBe("editorOpened");
    expect(seen).toEqual([
      {
        scope: SCOPE,
        skill: "mobile-app-release",
        action: "edit",
      },
    ]);
    expect(() =>
      native.extensions.openDetectedSkillResource(SCOPE, "../private", "edit"),
    ).toThrow();
  });

  it("copies one detected Skill using only stable ids", async () => {
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_detected_skill_copy`,
        async ({ request }) => {
          seen.push(await request.json());
          return HttpResponse.json([
            { ...wire, kind: "skill", id: "unity-cli" },
          ]);
        },
      ),
    );

    const refreshed = await native.extensions.copyDetectedSkill(
      SCOPE,
      "codex",
      "unity-cli",
    );
    expect(seen).toEqual([
      { scope: SCOPE, target: "codex", skill: "unity-cli" },
    ]);
    expect(refreshed[0]?.id).toBe("unity-cli");

    for (const bad of ["../private", "a\\b", "a\u0000b", ""]) {
      expect(() =>
        native.extensions.copyDetectedSkill(SCOPE, "codex", bad),
      ).toThrow();
    }
  });

  it("accepts an extension with no description", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () =>
        HttpResponse.json([{ ...wire, description: null }]),
      ),
    );
    const extensions = await native.extensions.list(SCOPE, "prompt");
    expect(extensions[0]?.description).toBeNull();
  });

  it("refuses a payload whose fields do not match the contract", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () =>
        HttpResponse.json([{ ...wire, canDisable: "yes" }]),
      ),
    );
    await expect(native.extensions.list(SCOPE, "mcp")).rejects.toBeInstanceOf(
      NativeError,
    );
  });

  it("strips a configuration payload instead of handing it to the UI", async () => {
    // `extensionSchema` isn't strict, so extra keys don't fail parsing — they
    // just get stripped. §36's structural guarantee actually relies on the
    // backend wire format never having these fields (pinned by the scan test
    // in domain/extension.rs); this test guards the fallback case — even if
    // the backend ever leaks config data, it never reaches the UI.
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () =>
        HttpResponse.json([
          {
            ...wire,
            server: { command: "npx", args: ["-y", "server-filesystem"] },
            content: "# House rules\nAlways run the tests.",
          },
        ]),
      ),
    );
    const [extension] = await native.extensions.list(SCOPE, "mcp");
    expect(extension).not.toHaveProperty("server");
    expect(extension).not.toHaveProperty("content");
    expect(JSON.stringify(extension)).not.toContain("npx");
    expect(JSON.stringify(extension)).not.toContain("House rules");
  });

  it("surfaces the backend error code instead of a string", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extension_set_enabled`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "EXTENSION_NOT_FOUND",
            messageKey: "error.extension.notFound",
            technicalMessage: "claude-code/mcp/filesystem",
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const error = await native.extensions
      .setEnabled(SCOPE, "mcp", "filesystem", true)
      .catch((thrown: unknown) => thrown);
    expect(error).toBeInstanceOf(NativeError);
    expect((error as NativeError).code).toBe("EXTENSION_NOT_FOUND");
  });
});
