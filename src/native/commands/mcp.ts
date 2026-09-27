import { invokeNative } from "../client";
import { operationIdSchema } from "../schemas/operation";
import {
  mcpEditFormSchema,
  mcpInstallDraftSchema,
  type McpEditForm,
  type McpInstallDraft,
} from "../schemas/mcp";
import {
  extensionListSchema,
  type Extension,
  type ExtensionScope,
} from "../schemas/extension";

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

  /**
   * The saved connection, values included, to prefill the edit form. Read
   * only while that form is open (ADR-0062).
   */
  get(scope: ExtensionScope, mcp: string): Promise<McpEditForm> {
    return invokeNative("app_mcp_get", mcpEditFormSchema, { scope, mcp });
  },

  /**
   * Save an edited connection under the same id. Every app keeps its switch;
   * the ones where it is on are rewritten. Returns this scope's list.
   */
  update(
    scope: ExtensionScope,
    mcp: string,
    draft: McpInstallDraft,
  ): Promise<Extension[]> {
    return invokeNative("app_mcp_update", extensionListSchema, {
      scope,
      mcp,
      draft: mcpInstallDraftSchema.parse(draft),
    });
  },
};
