import { renderHook, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { describe, expect, it, vi } from "vitest";
import {
  useInstallTool,
  useLaunchTool,
  useRepairTool,
  useUninstallTool,
  useUpdateTool,
} from "@/features/tool-management";
import { server } from "../../msw/server";
import {
  createTestQueryClient,
  withQueryClient,
} from "../../entities/queryWrapper";

const TAURI_ENDPOINT = "http://tauri.local";

const toastMocks = vi.hoisted(() => ({
  error: vi.fn(),
  info: vi.fn(),
  success: vi.fn(),
}));
vi.mock("sonner", () => ({ toast: toastMocks }));

describe("tool lifecycle mutations", () => {
  it("returns the operation id from an install", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_install`, () =>
        HttpResponse.json("op-1"),
      ),
    );
    const { result } = renderHook(() => useInstallTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate("claude-code");
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBe("op-1");
  });

  it("sends the update request for the tool it was given", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-2");
      }),
    );
    const { result } = renderHook(() => useUpdateTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate({
      tool: "opencode",
      previewFingerprint: "a".repeat(64),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(seen).toEqual([
      { tool: "opencode", previewFingerprint: "a".repeat(64) },
    ]);
  });

  it("starts a distinct repair operation", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_repair`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-repair");
      }),
    );
    const { result } = renderHook(() => useRepairTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate("codex");
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBe("op-repair");
    expect(seen).toEqual([{ tool: "codex" }]);
  });

  it("forwards the uninstall options exactly as chosen", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_uninstall`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("op-3");
      }),
    );
    const { result } = renderHook(() => useUninstallTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate({
      tool: "codex",
      options: { removeSettings: true, removeCache: false },
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(seen).toEqual([
      { tool: "codex", options: { removeSettings: true, removeCache: false } },
    ]);
  });

  it("returns the terminal handoff outcome from a launch", async () => {
    const seen: unknown[] = [];
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, async ({ request }) => {
        seen.push(await request.json());
        return HttpResponse.json("cancelled");
      }),
    );
    const { result } = renderHook(() => useLaunchTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate({ tool: "gemini-cli", directoryMode: "choose" });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBe("cancelled");
    expect(seen).toEqual([{ tool: "gemini-cli", directoryMode: "choose" }]);
  });

  it("shows launch remediation without exposing terminal diagnostics", async () => {
    toastMocks.error.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, () =>
        HttpResponse.json(
          {
            code: "LAUNCH_FAILED",
            messageKey: "error.tool.launchFailed",
            technicalMessage: "private/project/path and exit 1",
            remediation: "error.remediation.openToolManually",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    const { result } = renderHook(() => useLaunchTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate({ tool: "codex", directoryMode: "default" });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(toastMocks.error).toHaveBeenCalledWith("error.tool.launchFailed", {
      description: "error.remediation.openToolManually",
    });
    expect(JSON.stringify(toastMocks.error.mock.calls)).not.toContain(
      "private/project/path",
    );
  });

  it("lets a durable launch dialog suppress the transient error toast", async () => {
    toastMocks.error.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, () =>
        HttpResponse.json(
          {
            code: "LAUNCH_FAILED",
            messageKey: "error.tool.launchFailed",
            technicalMessage: "private/project/path and exit 1",
            remediation: "error.remediation.openToolManually",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    const { result } = renderHook(
      () => useLaunchTool({ notifyOnError: false }),
      { wrapper: withQueryClient(createTestQueryClient()) },
    );
    result.current.mutate({ tool: "codex", directoryMode: "default" });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(toastMocks.error).not.toHaveBeenCalled();
  });

  it("shows the translated message and remediation when the tool is busy", async () => {
    toastMocks.error.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, () =>
        HttpResponse.text(
          JSON.stringify({
            code: "OPERATION_CONFLICT",
            messageKey: "error.operation.toolBusy",
            technicalMessage: null,
            remediation: null,
            contextId: null,
          }),
          { status: 500 },
        ),
      ),
    );
    const { result } = renderHook(() => useUpdateTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate({
      tool: "claude-code",
      previewFingerprint: "a".repeat(64),
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    // i18n resources are empty in tests, so t() echoes the key back — the assertion is exactly "which key got rendered".
    expect(toastMocks.error).toHaveBeenCalledWith(
      "error.operation.toolBusy",
      expect.anything(),
    );
  });

  it("never puts a raw technical string in front of the user", async () => {
    toastMocks.error.mockClear();
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_install`, () =>
        HttpResponse.text("thread 'main' panicked at exit code 127", {
          status: 500,
        }),
      ),
    );
    const { result } = renderHook(() => useInstallTool(), {
      wrapper: withQueryClient(createTestQueryClient()),
    });
    result.current.mutate("codex");
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(toastMocks.error).toHaveBeenCalledWith(
      "error.native.unrecognized",
      expect.anything(),
    );
    expect(JSON.stringify(toastMocks.error.mock.calls)).not.toContain(
      "panicked",
    );
  });
});
