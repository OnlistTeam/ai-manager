import { z } from "zod";
import { invokeNative } from "../client";
import { operationIdSchema } from "../schemas/operation";
import {
  toolLaunchDirectoryModeSchema,
  toolLaunchOutcomeSchema,
  toolIdSchema,
  toolListSchema,
  toolUpdatePreviewListSchema,
  updatePreviewFingerprintSchema,
  toolUninstallPreviewSchema,
  toolVersionCatalogSchema,
  type Tool,
  type ToolId,
  type ToolLaunchOutcome,
  type ToolLaunchDirectoryMode,
  type ToolUpdatePreview,
  type ToolUninstallPreview,
  type ToolVersionCatalog,
} from "../schemas/tool";
import {
  uninstallOptionsSchema,
  UNINSTALL_APP_ONLY,
  type UninstallOptions,
} from "../schemas/uninstall";

export const tools = {
  /** Local-only read: installed or not, which version, whether it runs. Does not include the latest version. */
  list(): Promise<Tool[]> {
    return invokeNative("app_tools_list", toolListSchema);
  },

  /** The same inventory, plus the latest version looked up online; tools that can't be resolved still get `latestVersion: null`. */
  checkVersions(): Promise<Tool[]> {
    return invokeNative("app_tools_check_versions", toolListSchema);
  },

  /** Returns operationId immediately; progress is pushed via `onOperationChanged`. */
  install(tool: ToolId): Promise<string> {
    return invokeNative("app_tool_install", operationIdSchema, { tool });
  },

  updatePreview(tools: readonly ToolId[]): Promise<ToolUpdatePreview[]> {
    const parsed = z
      .array(toolIdSchema)
      .min(1)
      .max(toolIdSchema.options.length)
      .parse(tools);
    const request = [...new Set(parsed)];
    const responseSchema = toolUpdatePreviewListSchema.superRefine(
      (previews, context) => {
        const responseTools = previews.map((item) =>
          item.state === "ready" ? item.preview.tool : item.tool,
        );
        const matches =
          responseTools.length === request.length &&
          responseTools.every((tool, index) => tool === request[index]);
        if (!matches) {
          context.addIssue({
            code: "custom",
            message: "update preview response does not match requested tools",
          });
        }
      },
    );
    return invokeNative("app_tools_update_preview", responseSchema, {
      tools: request,
    });
  },

  update(tool: ToolId, previewFingerprint: string): Promise<string> {
    return invokeNative("app_tool_update", operationIdSchema, {
      tool,
      previewFingerprint:
        updatePreviewFingerprintSchema.parse(previewFingerprint),
    });
  },

  versionCatalog(tool: ToolId): Promise<ToolVersionCatalog> {
    return invokeNative("app_tool_version_catalog", toolVersionCatalogSchema, {
      tool,
    });
  },

  installVersion(tool: ToolId, version: string): Promise<string> {
    return invokeNative("app_tool_install_version", operationIdSchema, {
      tool,
      version: z.string().min(1).max(64).parse(version),
    });
  },

  repair(tool: ToolId): Promise<string> {
    return invokeNative("app_tool_repair", operationIdSchema, { tool });
  },

  /** Default directory or the native folder picker; the project path never crosses into the renderer. */
  launch(
    tool: ToolId,
    directoryMode: ToolLaunchDirectoryMode,
  ): Promise<ToolLaunchOutcome> {
    return invokeNative("app_tool_launch", toolLaunchOutcomeSchema, {
      tool,
      directoryMode: toolLaunchDirectoryModeSchema.parse(directoryMode),
    });
  },

  uninstallPreview(tool: ToolId): Promise<ToolUninstallPreview> {
    return invokeNative(
      "app_tool_uninstall_preview",
      toolUninstallPreviewSchema,
      {
        tool,
      },
    );
  },

  uninstall(
    tool: ToolId,
    options: UninstallOptions = UNINSTALL_APP_ONLY,
  ): Promise<string> {
    return invokeNative("app_tool_uninstall", operationIdSchema, {
      tool,
      options: uninstallOptionsSchema.parse(options),
    });
  },
};
