import { act, renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it } from "vitest";
import {
  toolKeys,
  useToolInventory,
  useTools,
  useToolUpdatePreviews,
} from "@/entities/tool";
import { server } from "../../msw/server";
import { createTestQueryClient, withQueryClient } from "../queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const TOOL = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "tools.description.claudeCode",
  status: "installed",
  version: "2.3.1",
  latestVersion: null,
  capabilities: {
    canInstall: true,
    canUpdate: true,
    canUninstall: true,
    canRepair: false,
    canManageProvider: true,
    canManageMcp: true,
    canManageSkills: true,
    canManagePrompts: true,
    canManageVersion: true,
    canLaunch: true,
  },
  sessionsInsideSettings: false,
  environment: null,
};

const READY_PREVIEW = {
  state: "ready",
  preview: {
    tool: "claude-code",
    previewFingerprint: "a".repeat(64),
    targetVersion: "2.4.0",
    source: "nativeInstaller",
    installations: [
      {
        source: "nativeInstaller",
        version: "2.3.1",
        runnable: true,
        isDefault: true,
        location: "/Users/test/.local/bin/claude",
      },
    ],
    attempts: [
      {
        method: "nativeSelfUpdate",
        commands: ["/Users/test/.local/bin/claude update"],
      },
    ],
    multipleInstallations: false,
  },
};

describe("useTools", () => {
  it("uses a prefix-invalidatable key", () => {
    expect(toolKeys.all).toEqual(["tools"]);
    expect(toolKeys.list().slice(0, 1)).toEqual(["tools"]);
  });

  it("returns the tool list from app_tools_list", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([TOOL]),
      ),
    );
    const { result } = renderHook(() => useTools(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([TOOL]);
  });

  it("reuses the session snapshot after remount and refreshes only on request", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        calls += 1;
        return HttpResponse.json([TOOL]);
      }),
    );
    const client = createTestQueryClient();
    const wrapper = withQueryClient(client);
    const first = renderHook(() => useTools(), { wrapper });
    await waitFor(() => expect(first.result.current.isSuccess).toBe(true));
    first.unmount();

    const second = renderHook(() => useTools(), { wrapper });
    await waitFor(() => expect(second.result.current.isSuccess).toBe(true));
    expect(calls).toBe(1);

    await act(async () => {
      await second.result.current.refetch();
    });
    expect(calls).toBe(2);
  });

  it("checks update details only while confirmation is open and always refetches", async () => {
    let calls = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_update_preview`, () => {
        calls += 1;
        return HttpResponse.json([READY_PREVIEW]);
      }),
    );
    const { result, rerender } = renderHook(
      ({ enabled }) => useToolUpdatePreviews(["claude-code"], enabled),
      {
        initialProps: { enabled: false },
        wrapper: withQueryClient(createTestQueryClient()),
      },
    );
    expect(calls).toBe(0);
    rerender({ enabled: true });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(calls).toBe(1);
    await act(async () => {
      await result.current.refetch();
    });
    expect(calls).toBe(2);
  });

  it("surfaces a structured NativeError instead of a raw string", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "INTERNAL",
            messageKey: "error.tool.actionPanicked",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const { result } = renderHook(() => useTools(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(result.current.error).toMatchObject({
      code: "INTERNAL",
      messageKey: "error.tool.actionPanicked",
    });
  });
});

describe("useToolInventory", () => {
  it("shows what is on disk first, then folds in the version check", async () => {
    let releaseCheck: (() => void) | null = null;
    const checkArrived = new Promise<void>((resolve) => {
      releaseCheck = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([TOOL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tools_check_versions`, async () => {
        await checkArrived;
        return HttpResponse.json([
          { ...TOOL, status: "updateAvailable", latestVersion: "2.4.0" },
        ]);
      }),
    );
    const { result } = renderHook(() => useToolInventory(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    // Render as soon as the local copy arrives: the latest version is still unknown, but the inventory is already fully readable.
    await waitFor(() => expect(result.current.data).toEqual([TOOL]));
    expect(result.current.isSuccess).toBe(true);
    expect(result.current.data?.[0]?.latestVersion).toBeNull();
    // Checking for a new version is its own thing — it doesn't count as "the inventory is refreshing", so it shouldn't lock the buttons in the UI.
    expect(result.current.checkingVersions).toBe(true);
    expect(result.current.isFetching).toBe(false);

    await act(async () => {
      releaseCheck?.();
      await checkArrived;
    });
    await waitFor(() =>
      expect(result.current.data?.[0]?.status).toBe("updateAvailable"),
    );
    expect(result.current.data?.[0]?.latestVersion).toBe("2.4.0");
    await waitFor(() => expect(result.current.checkingVersions).toBe(false));
  });

  it("keeps the local inventory whole when the version check fails", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([TOOL]),
      ),
      http.post(`${TAURI_ENDPOINT}/app_tools_check_versions`, () =>
        HttpResponse.text("registry unreachable", { status: 500 }),
      ),
    );
    const { result } = renderHook(() => useToolInventory(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    await waitFor(() => expect(result.current.checkingVersions).toBe(false));
    // If the latest version can't be resolved, stay silent about it: the inventory renders as usual, no error, and latestVersion stays empty.
    expect(result.current.data).toEqual([TOOL]);
    expect(result.current.data?.[0]?.latestVersion).toBeNull();
    expect(result.current.isError).toBe(false);
  });

  /// Right after installing a tool, the local inventory refreshes once; the
  /// previous round's version check was for a different version, and its
  /// result must not overwrite the new inventory — otherwise it would apply a
  /// stale answer to the new state.
  it("ignores a version check that describes a different install", async () => {
    let toolReads = 0;
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () => {
        toolReads += 1;
        return HttpResponse.json([
          { ...TOOL, version: toolReads === 1 ? "2.3.1" : "2.4.0" },
        ]);
      }),
      http.post(`${TAURI_ENDPOINT}/app_tools_check_versions`, () =>
        HttpResponse.json([
          { ...TOOL, status: "updateAvailable", latestVersion: "2.4.0" },
        ]),
      ),
    );
    const client = createTestQueryClient();
    const { result } = renderHook(() => useToolInventory(), {
      wrapper: withQueryClient(client),
    });
    await waitFor(() =>
      expect(result.current.data?.[0]?.status).toBe("updateAvailable"),
    );

    await act(async () => {
      await client.refetchQueries({ queryKey: toolKeys.list() });
    });
    expect(toolReads).toBe(2);
    await waitFor(() =>
      expect(result.current.data?.[0]?.version).toBe("2.4.0"),
    );
    expect(result.current.data?.[0]?.status).toBe("installed");
    expect(result.current.data?.[0]?.latestVersion).toBeNull();
  });
});
