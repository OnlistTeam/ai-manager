import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  extensionKeys,
  useExtensions,
  useLocalExtensionInventory,
} from "@/entities/extension";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const SCOPE = { kind: "tool", id: "claude-code" } as const;

const wire = {
  kind: "mcp",
  id: "filesystem",
  scope: SCOPE,
  name: "Filesystem",
  description: null,
  management: "managed",
  enabled: true,
  canDisable: true,
};

describe("extensionKeys", () => {
  it("keeps each tool and kind on its own cache entry", () => {
    expect(extensionKeys.list("claude-code", "mcp")).toEqual([
      "extensions",
      "list",
      "claude-code",
      "mcp",
    ]);
    expect(extensionKeys.list("claude-code", "mcp")).not.toEqual(
      extensionKeys.list("claude-code", "skill"),
    );
    expect(extensionKeys.list("claude-code", "mcp")).not.toEqual(
      extensionKeys.list("codex", "mcp"),
    );
    expect(extensionKeys.all).toEqual(["extensions"]);
    expect(extensionKeys.localInventory()).toEqual([
      "extensions",
      "local-inventory",
    ]);
  });
});

describe("useLocalExtensionInventory", () => {
  it("reads every safe local scope through one query", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_local_inventory`, () =>
        HttpResponse.json({
          items: [{ ...wire, management: "detected" }],
          scopes: [{ tool: "claude-code", kind: "mcp", status: "ready" }],
          truncated: false,
        }),
      ),
    );
    const { result } = renderHook(() => useLocalExtensionInventory(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.items[0]?.name).toBe("Filesystem");
  });
});

describe("useExtensions", () => {
  it("reads one kind for one tool once per session across page remounts", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () => {
        calls += 1;
        return HttpResponse.json([wire]);
      }),
    );
    const client = createTestQueryClient();
    const first = renderHook(() => useExtensions(SCOPE, "mcp"), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    expect(first.result.current.data?.[0]?.name).toBe("Filesystem");
    first.unmount();

    const second = renderHook(() => useExtensions(SCOPE, "mcp"), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(second.result.current.data?.[0]?.name).toBe("Filesystem");
    expect(calls).toBe(1);
  });

  it("asks for nothing until a tool is known", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () => {
        calls += 1;
        return HttpResponse.json([]);
      }),
    );
    const { result } = renderHook(() => useExtensions(null, "mcp"), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isPending).toBe(true));
    expect(calls).toBe(0);
  });

  it("reports a failure instead of an empty list", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_list`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "UPSTREAM_ERROR",
            messageKey: "error.extension.listFailed",
            technicalMessage: null,
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const { result } = renderHook(() => useExtensions(SCOPE, "skill"), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
  });
});
