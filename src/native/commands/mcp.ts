import { invokeNative } from "../client";
import { operationIdSchema } from "../schemas/operation";
import { mcpInstallDraftSchema, type McpInstallDraft } from "../schemas/mcp";
import type { ExtensionScope } from "../schemas/extension";

export const mcp = {
  install(scope: ExtensionScope, draft: McpInstallDraft): Promise<string> {
    return invokeNative("app_mcp_install", operationIdSchema, {
      scope,
      draft: mcpInstallDraftSchema.parse(draft),
    });
  },

  remove(scope: ExtensionScope, mcp: string): Promise<string> {
    return invokeNative("app_mcp_remove", operationIdSchema, { scope, mcp });
  },
};
