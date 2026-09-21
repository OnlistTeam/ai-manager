import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { QueryClient } from "@tanstack/react-query";
import { extensionKeys } from "@/entities/extension";
import { healthKeys } from "@/entities/health";
import {
  useCopyDetectedSkill,
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

describe("useCopyDetectedSkill", () => {
  const skillWire = { ...wire, kind: "skill", id: "unity-cli" };

  function mountCopy(client: QueryClient) {
    return renderHook(() => useCopyDetectedSkill(), {
      wrapper: withQueryClient(client),
    });
  }

  it("writes the target tool's authoritative list and refreshes what the copy changed", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_detected_skill_copy`, () =>
        HttpResponse.json([skillWire]),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mountCopy(client);

    result.current.mutate({
      scope: SCOPE,
      target: "codex",
      skillId: "unity-cli",
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(
      client.getQueryData(extensionKeys.list("tool:codex", "skill")),
    ).toEqual([skillWire]);
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: extensionKeys.list("tool:claude-code", "skill"),
    });
    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: extensionKeys.localInventory(),
    });
  });

  it("re-reads both tools and the local inventory when the copy fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_detected_skill_copy`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "CONFIG_WRITE_FAILED",
            messageKey: "error.extension.copyFailed",
            technicalMessage: "/Users/somebody/.codex/skills",
            remediation: "error.remediation.retryOrViewDetails",
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const client = createTestQueryClient();
    const invalidateSpy = vi.spyOn(client, "invalidateQueries");
    const { result } = mountCopy(client);

    result.current.mutate({
      scope: SCOPE,
      target: "codex",
      skillId: "unity-cli",
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    for (const queryKey of [
      extensionKeys.list("tool:codex", "skill"),
      extensionKeys.list("tool:claude-code", "skill"),
      extensionKeys.localInventory(),
    ]) {
      expect(invalidateSpy).toHaveBeenCalledWith({ queryKey });
    }
    expect(toastMocks.error).not.toHaveBeenCalled();
  });
});
