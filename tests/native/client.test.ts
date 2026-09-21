import { http, HttpResponse } from "msw";
import { server } from "../msw/server";
import { emitTauriEvent } from "../msw/tauriMocks";
import {
  NativeError,
  native,
  onOperationChanged,
  OPERATION_CHANGED_EVENT,
} from "@/native";

const TAURI_ENDPOINT = "http://tauri.local";

const toolFixture = {
  id: "claude-code",
  name: "Claude Code",
  descriptionKey: "tool.claude-code.description",
  discovery: {
    publisher: "Anthropic",
    access: "vendorOrProvider",
    useCases: ["officialCoding"],
  },
  status: "updateAvailable",
  version: "2.1.0",
  latestVersion: "2.2.0",
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
  environment: "macos",
};

const operationFixture = {
  id: "op-1",
  kind: "install",
  tool: "codex",
  status: "running",
  progress: 42,
  messageKey: "operation.install.downloading",
  logs: [
    {
      timestamp: 1760000000100,
      kind: "command",
      messageKey: null,
      detail: "npm install -g @openai/codex@latest",
    },
  ],
  error: null,
  startedAt: 1760000000000,
  finishedAt: null,
};

describe("native client", () => {
  it("pins the operation changed event name literal", () => {
    expect(OPERATION_CHANGED_EVENT).toBe("operation://changed");
  });

  it("parses the tool list returned by app_tools_list", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([
          toolFixture,
          {
            ...toolFixture,
            id: "codex",
            version: null,
            latestVersion: null,
            environment: null,
          },
        ]),
      ),
    );

    const tools = await native.tools.list();

    expect(tools).toHaveLength(2);
    expect(tools[0].id).toBe("claude-code");
    expect(tools[0].status).toBe("updateAvailable");
    expect(tools[0].capabilities.canManageSkills).toBe(true);
    expect(tools[0].discovery?.publisher).toBe("Anthropic");
    expect(tools[0].environment).toBe("macos");
    expect(tools[1].version).toBeNull();
    expect(tools[1].environment).toBeNull();
  });

  it("reads a bounded version catalog and sends only the stable tool id", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_version_catalog`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            tool: "codex",
            source: "pnpm",
            canChangeVersion: true,
            restriction: null,
            latestVersion: "1.2.0",
            distTags: [
              { tag: "latest", version: "1.2.0" },
              { tag: "beta", version: "1.3.0-beta.1" },
            ],
            versions: ["1.2.0", "1.1.0"],
            mirrorUsed: false,
          });
        },
      ),
    );

    const catalog = await native.tools.versionCatalog("codex");
    expect(body).toEqual({ tool: "codex" });
    expect(catalog.source).toBe("pnpm");
    expect(catalog.distTags).toEqual([
      { tag: "latest", version: "1.2.0" },
      { tag: "beta", version: "1.3.0-beta.1" },
    ]);
    expect(catalog.versions).toEqual(["1.2.0", "1.1.0"]);
  });

  it("reads an exact uninstall preview but sends only the stable tool id", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_uninstall_preview`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            tool: "gemini-cli",
            app: [
              {
                kind: "command",
                value: "npm uninstall -g @google/gemini-cli",
                canRemoveAutomatically: true,
              },
            ],
            settings: [
              {
                kind: "directory",
                value: "/Users/test/.gemini",
                canRemoveAutomatically: true,
              },
            ],
            cache: [],
          });
        },
      ),
    );

    const preview = await native.tools.uninstallPreview("gemini-cli");

    expect(body).toEqual({ tool: "gemini-cli" });
    expect(preview.app[0].kind).toBe("command");
    expect(preview.settings[0].value).toBe("/Users/test/.gemini");
    expect(preview.cache).toEqual([]);
  });

  it("accepts only the native-proven Hermes Python owner source tags", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tool_version_catalog`, () =>
        HttpResponse.json({
          tool: "hermes",
          source: "uv",
          canChangeVersion: true,
          restriction: null,
          latestVersion: "0.19.0",
          distTags: [{ tag: "latest", version: "0.19.0" }],
          versions: ["0.19.0", "0.18.0"],
          mirrorUsed: false,
        }),
      ),
    );

    await expect(native.tools.versionCatalog("hermes")).resolves.toMatchObject({
      source: "uv",
      canChangeVersion: true,
    });
  });

  it("sends an exact validated version when starting a version change", async () => {
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_tool_install_version`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json("op-version-1");
        },
      ),
    );

    await expect(
      native.tools.installVersion("claude-code", "2.1.0-beta.1"),
    ).resolves.toBe("op-version-1");
    expect(body).toEqual({
      tool: "claude-code",
      version: "2.1.0-beta.1",
    });
    expect(() => native.tools.installVersion("claude-code", "")).toThrow();
  });

  it("rejects payloads that do not match the schema", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json([{ ...toolFixture, status: "totally-unknown" }]),
      ),
    );

    await expect(native.tools.list()).rejects.toBeInstanceOf(NativeError);
    await expect(native.tools.list()).rejects.toMatchObject({
      code: "INTERNAL",
      messageKey: "error.native.responseSchemaMismatch",
    });
  });

  it("maps a structured Rust AppError onto NativeError", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.json(
          {
            code: "TOOL_NOT_FOUND",
            messageKey: "error.tool.notFound",
            technicalMessage: "which claude -> exit 1",
            remediation: "error.remediation.installClaude",
            contextId: "op-7",
          },
          { status: 500 },
        ),
      ),
    );

    const error = await native.tools.list().catch((raw: unknown) => raw);

    expect(error).toBeInstanceOf(NativeError);
    expect(error).toMatchObject({
      code: "TOOL_NOT_FOUND",
      messageKey: "error.tool.notFound",
      technicalMessage: "which claude -> exit 1",
      remediation: "error.remediation.installClaude",
      contextId: "op-7",
    });
  });

  it("falls back to INTERNAL for unrecognised rejections", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_tools_list`, () =>
        HttpResponse.text("boom", { status: 500 }),
      ),
    );

    const error = await native.tools.list().catch((raw: unknown) => raw);

    expect(error).toBeInstanceOf(NativeError);
    expect(error).toMatchObject({
      code: "INTERNAL",
      messageKey: "error.native.unrecognized",
    });
  });

  it("parses the operation list returned by app_operations_list", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([operationFixture]),
      ),
    );

    const operations = await native.operations.list();

    expect(operations).toHaveLength(1);
    expect(operations[0].kind).toBe("install");
    expect(operations[0].progress).toBe(42);
    expect(operations[0].logs[0].detail).toContain("@openai/codex");
    expect(operations[0].error).toBeNull();
    expect(operations[0].updateRecovery).toBeNull();
    expect(operations[0].output).toBeNull();
    expect(operations[0].canCancel).toBe(false);
  });

  it("parses bounded typed provider results from an operation", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([
          {
            ...operationFixture,
            kind: "testProviders",
            tool: "claude-code",
            output: {
              kind: "providerTests",
              results: [
                {
                  providerId: "relay",
                  reachability: "operational",
                  responseTimeMs: 120,
                  httpStatus: 401,
                },
              ],
            },
          },
        ]),
      ),
    );

    await expect(native.operations.list()).resolves.toMatchObject([
      {
        kind: "testProviders",
        output: {
          kind: "providerTests",
          results: [{ providerId: "relay", httpStatus: 401 }],
        },
      },
    ]);
  });

  it("rejects provider output on the wrong operation or with duplicate ids", async () => {
    const providerOutput = {
      kind: "providerTests",
      results: [
        {
          providerId: "relay",
          reachability: "failed",
          responseTimeMs: null,
          httpStatus: null,
        },
      ],
    };
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([{ ...operationFixture, output: providerOutput }]),
      ),
    );
    await expect(native.operations.list()).rejects.toBeInstanceOf(NativeError);

    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([
          {
            ...operationFixture,
            kind: "testProviders",
            output: {
              ...providerOutput,
              results: [providerOutput.results[0], providerOutput.results[0]],
            },
          },
        ]),
      ),
    );
    await expect(native.operations.list()).rejects.toBeInstanceOf(NativeError);
  });

  it("cancels only a canonical operation id and parses the native gate update", async () => {
    const id = "123e4567-e89b-42d3-a456-426614174000";
    let body: unknown;
    server.use(
      http.post(
        `${TAURI_ENDPOINT}/app_operation_cancel`,
        async ({ request }) => {
          body = await request.json();
          return HttpResponse.json({
            ...operationFixture,
            id,
            messageKey: "operation.phase.cancelling",
            canCancel: false,
          });
        },
      ),
    );

    await expect(native.operations.cancel(id)).resolves.toMatchObject({
      id,
      status: "running",
      canCancel: false,
      messageKey: "operation.phase.cancelling",
    });
    expect(body).toEqual({ operationId: id });
    expect(() => native.operations.cancel("op-1")).toThrow();
  });

  it("accepts only a bounded exact failed-update recovery target", async () => {
    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([
          {
            ...operationFixture,
            kind: "update",
            status: "failed",
            updateRecovery: {
              kind: "available",
              targetVersion: "1.2.3-beta.1",
            },
          },
        ]),
      ),
    );
    await expect(native.operations.list()).resolves.toMatchObject([
      {
        updateRecovery: {
          kind: "available",
          targetVersion: "1.2.3-beta.1",
        },
      },
    ]);

    server.use(
      http.post(`${TAURI_ENDPOINT}/app_operations_list`, () =>
        HttpResponse.json([
          {
            ...operationFixture,
            kind: "update",
            status: "failed",
            updateRecovery: {
              kind: "available",
              targetVersion: "1.2.3; unsafe",
            },
          },
        ]),
      ),
    );
    await expect(native.operations.list()).rejects.toBeInstanceOf(NativeError);
  });

  it("delivers validated operation events and drops malformed ones", async () => {
    const received: unknown[] = [];
    const unlisten = await onOperationChanged((operation) => {
      received.push(operation);
    });

    emitTauriEvent(OPERATION_CHANGED_EVENT, operationFixture);
    emitTauriEvent(OPERATION_CHANGED_EVENT, { id: "broken" });

    expect(received).toHaveLength(1);
    expect(received[0]).toMatchObject({ id: "op-1", status: "running" });

    unlisten();

    // Verify unlisten stops event delivery
    emitTauriEvent(OPERATION_CHANGED_EVENT, operationFixture);
    expect(received).toHaveLength(1);
  });
});
