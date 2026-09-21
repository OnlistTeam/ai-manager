import { toolIdSchema, type ToolId } from "@/native";

export {
  toolKeys,
  toolsQueryOptions,
  toolVersionCheckQueryOptions,
  useTools,
  useToolInventory,
  useToolUpdatePreviews,
  useToolUninstallPreview,
  useToolVersionCatalog,
  useToolVersionCheck,
} from "./queries";
export type { ToolInventory } from "./queries";
export { hasManageableConfiguration } from "./scope";
export type {
  Tool,
  ToolAccessRequirement,
  ToolCapabilities,
  ToolDiscovery,
  ToolId,
  ToolInstallSource,
  ToolStatus,
  ToolUpdateAttemptPreview,
  ToolUpdateBlockReason,
  ToolUpdateInstallation,
  ToolUpdateMethod,
  ToolUpdatePreview,
  ToolUpdateReadyPreview,
  ToolUninstallPreview,
  ToolUninstallTarget,
  ToolUninstallTargetKind,
  ToolUseCase,
  ToolVersionCatalog,
  ToolVersionRestriction,
  ToolVersionTag,
} from "@/native";

export const TOOL_IDS: readonly ToolId[] = toolIdSchema.options;

export function isToolId(value: string): value is ToolId {
  return (TOOL_IDS as readonly string[]).includes(value);
}
