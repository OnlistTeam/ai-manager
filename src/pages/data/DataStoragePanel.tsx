import { AlertCircle, HardDrive } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  toolExtensionScope,
  type ExtensionKind,
  type ExtensionScope,
} from "@/entities/extension";
import type { Tool, ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import { InventoryRefreshNotice } from "@/shared/ui/InventoryRefreshNotice";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import type { DataInventoryState } from "./DataOverviewStrip";
import { ProviderRuntimeContextPanel } from "./ProviderRuntimeContextPanel";

export interface DataStoragePanelProps {
  installed: readonly Tool[];
  inventory: DataInventoryState;
  /** The inventory was read once successfully, but the most recent refresh failed: keep showing the previous copy and note it. */
  inventoryRefreshFailed: boolean;
  inventoryRefreshing: boolean;
  activeTool: ToolId | null;
  activeSupported: boolean;
  onRetryInventory: () => void;
  onOpenExtensions?: (scope?: ExtensionScope, kind?: ExtensionKind) => void;
}

/** The always-open resource panel for whichever tool is selected by the page's scope tabs. */
export function DataStoragePanel({
  installed,
  inventory,
  inventoryRefreshFailed,
  inventoryRefreshing,
  activeTool,
  activeSupported,
  onRetryInventory,
  onOpenExtensions,
}: DataStoragePanelProps) {
  const { t } = useTranslation();
  const active = installed.find((tool) => tool.id === activeTool) ?? null;
  const activeToolName = active?.name ?? null;
  /**
   * Hand off only when the product itself is capable of managing this global
   * prompt; otherwise this is its only entry point (ADR-0037). The check is
   * a capability flag, not a tool name (AI_RULES rule 8).
   */
  const manageInstructions =
    active !== null &&
    active.capabilities.canManagePrompts &&
    onOpenExtensions !== undefined
      ? () => onOpenExtensions(toolExtensionScope(active.id), "prompt")
      : null;

  return (
    <div className="flex flex-col gap-4">
      <SectionHeader
        as="h2"
        title={t("data.storage.title")}
        description={t("data.storage.description")}
      />

      {inventory === "pending" ? (
        <DetectionStatus label={t("data.storage.loading")} />
      ) : null}

      {inventory === "unavailable" ? (
        <div
          role="alert"
          aria-label={t("data.inventory.error.title")}
          aria-busy={inventoryRefreshing || undefined}
        >
          <EmptyState
            icon={AlertCircle}
            title={t("data.inventory.error.title")}
            description={t("data.inventory.error.description")}
            action={
              <Button loading={inventoryRefreshing} onClick={onRetryInventory}>
                {t("data.inventory.retry")}
              </Button>
            }
          />
        </div>
      ) : null}

      {inventoryRefreshFailed ? (
        <InventoryRefreshNotice
          title={t("data.inventory.refreshError.title")}
          description={t("data.inventory.refreshError.description")}
          actionLabel={t("data.inventory.retry")}
          refreshing={inventoryRefreshing}
          onRetry={onRetryInventory}
        />
      ) : null}

      {inventory === "ready" && installed.length === 0 ? (
        <EmptyState
          icon={HardDrive}
          title={t("data.storage.empty.title")}
          description={t("data.storage.empty.description")}
        />
      ) : null}

      {active && !activeSupported ? (
        <EmptyState
          icon={HardDrive}
          title={t("data.scope.unsupportedTitle", { tool: active.name })}
          description={t("data.scope.unsupportedDescription", {
            tool: active.name,
          })}
        />
      ) : null}

      {installed.length > 0 && activeSupported ? (
        <ProviderRuntimeContextPanel
          tool={activeTool}
          toolName={activeToolName}
          onManageInstructions={manageInstructions}
        />
      ) : null}
    </div>
  );
}
