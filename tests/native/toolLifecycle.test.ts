import { http, HttpResponse, type JsonBodyType } from "msw";
import { server } from "../msw/server";
import { NativeError, native } from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";

const READY_UPDATE_PREVIEW = {
  state: "ready",
  preview: {
    tool: "claude-code",
    previewFingerprint: "a".repeat(64),
    targetVersion: "2.1.212",
    source: "nativeInstaller",
    installations: [
      {
        source: "nativeInstaller",
        version: "2.1.211",
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
      {
        method: "officialInstaller",
        commands: ["bash -c [official installer]"],
      },
    ],
    multipleInstallations: false,
  },
} as const;

/** Captures the payload actually sent by invoke, to verify the frontend hasn't silently renamed fields. */
function capture(command: string, response: JsonBodyType) {
  const seen: Record<string, unknown>[] = [];
  server.use(
    http.post(`${TAURI_ENDPOINT}/${command}`, async ({ request }) => {
      seen.push((await request.json()) as Record<string, unknown>);
      return HttpResponse.json(response);
    }),
  );
  return seen;
}

describe("native tool lifecycle", () => {
  it("returns the operation id from app_tool_install", async () => {
    const seen = capture("app_tool_install", "op-install-1");
    await expect(native.tools.install("claude-code")).resolves.toBe(
      "op-install-1",
    );
    expect(seen).toEqual([{ tool: "claude-code" }]);
  });

  it("returns the operation id from app_tool_update", async () => {
    const seen = capture("app_tool_update", "op-update-1");
    await expect(native.tools.update("opencode", "a".repeat(64))).resolves.toBe(
      "op-update-1",
    );
    expect(seen).toEqual([
      { tool: "opencode", previewFingerprint: "a".repeat(64) },
    ]);
  });

  it("parses ready and blocked update previews without accepting executable input", async () => {
    const response = [
      READY_UPDATE_PREVIEW,
      { state: "blocked", tool: "codex", reason: "inspectionFailed" },
    ];
    const seen = capture("app_tools_update_preview", response);
    await expect(
      native.tools.updatePreview(["claude-code", "codex"]),
    ).resolves.toEqual(response);
    expect(seen).toEqual([{ tools: ["claude-code", "codex"] }]);
  });

  it("keeps executable update decisions native-owned", async () => {
    capture("app_tools_update_preview", [READY_UPDATE_PREVIEW]);
    const previews = await native.tools.updatePreview(["claude-code"]);
    const json = JSON.stringify(previews);
    for (const forbidden of [
      "programPath",
      "sensitiveArgIndices",
      "registry",
      "packageName",
      "realPath",
      '"args"',
      '"env"',
    ]) {
      expect(json).not.toContain(forbidden);
    }
  });

  it("rejects update previews that do not match the requested tools", async () => {
    capture("app_tools_update_preview", [
      {
        ...READY_UPDATE_PREVIEW,
        preview: { ...READY_UPDATE_PREVIEW.preview, tool: "codex" },
      },
    ]);
    await expect(
      native.tools.updatePreview(["claude-code"]),
    ).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("rejects unknown update preview fields", async () => {
    capture("app_tools_update_preview", [
      {
        state: "blocked",
        tool: "codex",
        reason: "inspectionFailed",
        command: "rm -rf /",
      },
    ]);
    await expect(native.tools.updatePreview(["codex"])).rejects.toMatchObject({
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("launches a tool without sending a project path through the renderer", async () => {
    const seen = capture("app_tool_launch", "launched");
    await expect(native.tools.launch("gemini-cli", "default")).resolves.toBe(
      "launched",
    );
    expect(seen).toEqual([{ tool: "gemini-cli", directoryMode: "default" }]);
  });

  it("treats folder-picker cancellation as a valid launch outcome", async () => {
    capture("app_tool_launch", "cancelled");
    await expect(native.tools.launch("codex", "choose")).resolves.toBe(
      "cancelled",
    );
  });

  it("rejects an unknown launch outcome", async () => {
    capture("app_tool_launch", "opened-somehow");
    await expect(native.tools.launch("codex", "default")).rejects.toMatchObject(
      {
        code: "INTERNAL",
        messageKey: "error.native.responseSchemaMismatch",
      },
    );
  });

  it("surfaces terminal handoff failures as structured LAUNCH_FAILED errors", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_launch`, () =>
        HttpResponse.json(
          {
            code: "LAUNCH_FAILED",
            messageKey: "error.tool.launchFailed",
            technicalMessage: "Terminal bridge exited with code 1",
            remediation: "error.remediation.openToolManually",
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    await expect(native.tools.launch("codex", "default")).rejects.toMatchObject(
      {
        code: "LAUNCH_FAILED",
        messageKey: "error.tool.launchFailed",
        remediation: "error.remediation.openToolManually",
      },
    );
  });

  it("uninstalls the app only unless the caller opts in", async () => {
    const seen = capture("app_tool_uninstall", "op-uninstall-1");
    await expect(native.tools.uninstall("codex")).resolves.toBe(
      "op-uninstall-1",
    );
    expect(seen).toEqual([
      { tool: "codex", options: { removeSettings: false, removeCache: false } },
    ]);
  });

  it("forwards both removal flags when they are set", async () => {
    const seen = capture("app_tool_uninstall", "op-uninstall-2");
    await native.tools.uninstall("codex", {
      removeSettings: true,
      removeCache: true,
    });
    expect(seen).toEqual([
      { tool: "codex", options: { removeSettings: true, removeCache: true } },
    ]);
  });

  it("rejects a response that is not an operation id", async () => {
    capture("app_tool_install", { id: "op-1" });
    await expect(native.tools.install("codex")).rejects.toMatchObject({
      code: "INTERNAL",
      messageKey: "error.native.responseSchemaMismatch",
    });
    capture("app_tool_install", "");
    await expect(native.tools.install("codex")).rejects.toBeInstanceOf(
      NativeError,
    );
  });

  it("surfaces a busy tool as a structured OPERATION_CONFLICT", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_update`, () =>
        HttpResponse.json(
          {
            code: "OPERATION_CONFLICT",
            messageKey: "error.operation.toolBusy",
            technicalMessage: "codex is already mutating",
            remediation: null,
            contextId: null,
          },
          { status: 500 },
        ),
      ),
    );
    const error = await native.tools
      .update("codex", "a".repeat(64))
      .catch((raw: unknown) => raw);
    expect(error).toBeInstanceOf(NativeError);
    expect(error).toMatchObject({
      code: "OPERATION_CONFLICT",
      messageKey: "error.operation.toolBusy",
    });
  });
});
