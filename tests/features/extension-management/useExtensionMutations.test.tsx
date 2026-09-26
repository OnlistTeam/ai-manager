import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import {
  useAdoptDetected,
  useSetExtensionEnabled,
} from "@/features/extension-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";
const SCOPE = { kind: "tool", id: "claude-code" } as const;

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

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

function mount(client: QueryClient) {
  return renderHook(() => useSetExtensionEnabled(), {
    wrapper: withQueryClient(client),
  });
}

describe("useSetExtensionEnabled", () => {
  beforeEach(() => {
    toastMocks.error.mockClear();
  });

  it("writes the refreshed list straight into the cache", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extension_set_enabled`, () =>
        HttpResponse.json([wire]),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mount(client);

    result.current.mutate({
      scope: SCOPE,
      kind: "mcp",
      extensionId: "filesystem",
      enabled: true,
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(
      client.getQueryData(extensionKeys.list("tool:claude-code", "mcp")),
    ).toEqual([wire]);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
  });

  it("invalidates authoritative extension state without a transient error toast", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extension_set_enabled`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.extension.toggleFailed",
            technicalMessage: "permission denied: /Users/somebody/.claude",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mount(client);

    result.current.mutate({
      scope: SCOPE,
      kind: "mcp",
      extensionId: "filesystem",
      enabled: false,
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: extensionKeys.list("tool:claude-code", "mcp"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
    expect(toastMocks.error).not.toHaveBeenCalled();
  });
});

describe("useAdoptDetected", () => {
  function mountAdopt(client: QueryClient) {
    return renderHook(() => useAdoptDetected(), {
      wrapper: withQueryClient(client),
    });
  }

  it("takes the item over from each app, then sets the clicked one", async () => {
    const calls: unknown[] = [];
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_extensions_adopt_detected`,
        async ({ request }) => {
          calls.push(await request.json());
          return HttpResponse.json([]);
        },
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mountAdopt(client);
    const desktop = { kind: "desktopApp", id: "claude-desktop" } as const;
    const codex = { kind: "tool", id: "codex" } as const;

    result.current.mutate({
      kind: "mcp",
      extensionId: "context7",
      found: [SCOPE, desktop],
      scope: codex,
      enabled: true,
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(calls).toEqual([
      { scope: SCOPE, kind: "mcp", extension: "context7", enabled: true },
      { scope: desktop, kind: "mcp", extension: "context7", enabled: true },
      { scope: codex, kind: "mcp", extension: "context7", enabled: true },
    ]);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: extensionKeys.all,
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: healthKeys.snapshots,
    });
  });

  it("stops at the first failing app and still re-reads what changed", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_extensions_adopt_detected`, () => {
        calls += 1;
        return HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.extension.adoptFailed",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        );
      }),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mountAdopt(client);

    result.current.mutate({
      kind: "skill",
      extensionId: "local",
      found: [SCOPE, { kind: "tool", id: "codex" }],
      scope: SCOPE,
      enabled: false,
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(calls).toBe(1);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: extensionKeys.all,
    });
  });
});
