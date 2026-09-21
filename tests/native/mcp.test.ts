import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it } from "vitest";
import { native, NativeError, type McpInstallDraft } from "@/native";
import { server } from "../msw/server";

const TAURI_ENDPOINT = "http://tauri.local";
const CLAUDE_SCOPE = { kind: "tool", id: "claude-code" } as const;

describe("native.mcp", () => {
  let seen: unknown[];

  beforeEach(() => {
    seen = [];
  });

  it("starts a guided background install without a renderer-owned id or config JSON", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-mcp-1");
      }),
    );
    const draft: McpInstallDraft = {
      name: "Filesystem",
      description: "Reads selected files.",
      connection: {
        transport: "stdio",
        command: "npx",
        arguments: ["-y", "server-filesystem"],
      },
    };

    await expect(native.mcp.install(CLAUDE_SCOPE, draft)).resolves.toBe(
      "op-mcp-1",
    );
    expect(seen).toEqual([{ scope: CLAUDE_SCOPE, draft }]);
    expect(seen[0]).not.toHaveProperty("id");
    expect(JSON.stringify(seen)).not.toContain('"server"');
  });

  it("rejects secret-bearing free-form fields before native IPC", () => {
    const unsafe = {
      name: "Private server",
      description: null,
      connection: {
        transport: "http",
        url: "https://mcp.example.test",
        headers: { Authorization: "Bearer secret" },
      },
    } as unknown as McpInstallDraft;

    expect(() =>
      native.mcp.install({ kind: "tool", id: "codex" }, unsafe),
    ).toThrow();
    expect(seen).toEqual([]);
  });

  it("starts a destructive removal with only the tool and stable connection id", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_remove`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-mcp-remove-1");
      }),
    );

    await expect(
      native.mcp.remove(
        { kind: "desktopApp", id: "claude-desktop" },
        "filesystem-a1b2c3d4",
      ),
    ).resolves.toBe("op-mcp-remove-1");
    expect(seen).toEqual([
      {
        scope: { kind: "desktopApp", id: "claude-desktop" },
        mcp: "filesystem-a1b2c3d4",
      },
    ]);
  });

  it("keeps structured backend failures", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_mcp_install`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.mcp.installFailed",
            technicalMessage: "redacted detail",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const error = await native.mcp
      .install(
        { kind: "tool", id: "codex" },
        {
          name: "Docs",
          description: null,
          connection: {
            transport: "http",
            url: "https://mcp.example.test",
          },
        },
      )
      .catch((thrown: unknown) => thrown);

    expect(error).toBeInstanceOf(NativeError);
    expect((error as NativeError).code).toBe("CONFIG_WRITE_FAILED");
  });
});
