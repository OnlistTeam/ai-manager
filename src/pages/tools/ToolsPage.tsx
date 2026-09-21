import { useState } from "react";
import { useTranslation } from "react-i18next";
import { type DesktopApp } from "@/entities/desktop-app";
import { type Operation } from "@/entities/operation";
import { type Tool, type ToolId } from "@/entities/tool";
import {
  TaskAvailabilityNotice,
  useTaskAvailability,
} from "@/features/task-center";
import {
  retryActionForOperation,
  toolCardOperations,
  toolsAwaitingInventory,
  busyTools,
  useCancelOperation,
  useToolLaunchFlow,
  useUninstallTool,
} from "@/features/tool-management";
import type { ExtensionScope, ExtensionKind } from "@/entities/extension";
import { ToolDetailsModal } from "./ToolDetailsModal";
import { focusToolCard } from "./focusToolCard";
import type { ToolCardAction } from "@/shared/ui/ToolCard";
import { ToolsPageDialogs } from "./ToolsPageDialogs";
import { ToolsPageInventory } from "./ToolsPageInventory";
import { DesktopAppsSection } from "./DesktopAppsSection";
import { ToolsGrid } from "./ToolsGrid";
import { useToolInstallFlow } from "./useToolInstallFlow";
import { useToolInventoryRecovery } from "./useToolInventoryRecovery";
import { useToolUpdateFlow } from "./useToolUpdateFlow";
import { useToolVersionFlows } from "./useToolVersionFlows";

export interface ToolsPageProps {
  onOpenServices?: (tool?: ToolId) => void;
  onOpenExtensions?: (scope?: ExtensionScope, kind?: ExtensionKind) => void;
  preferredToolId?: ToolId | null;
}

export function ToolsPage({
  onOpenServices,
  onOpenExtensions,
  preferredToolId = null,
}: ToolsPageProps = {}) {
  const { t } = useTranslation();
  const {
    tools,
    pageRef,
    inventoryInitiallyLoading,
    inventoryUnavailable,
    inventoryRefreshFailed,
    inventoryActionsBlocked,
    checkingVersions,
    retryInventory,
  } = useToolInventoryRecovery();
  const {
    operations,
    state: taskAvailability,
    actionsBlocked: taskActionsBlocked,
  } = useTaskAvailability();
  const actionsBlocked = inventoryActionsBlocked || taskActionsBlocked;
  const install = useToolInstallFlow();
  const cancelOperation = useCancelOperation();
  const uninstall = useUninstallTool({ notifyOnError: false });
  const launch = useToolLaunchFlow();
  const [removing, setRemoving] = useState<Tool | null>(null);
  const [detailsId, setDetailsId] = useState<ToolId | null>(preferredToolId);
  const detailsTool = tools.data?.find(
    (tool) => tool.id === detailsId && tool.status !== "notInstalled",
  );
  const [dismissedFailures, setDismissedFailures] = useState<
    ReadonlySet<string>
  >(() => new Set());

  const operationList = operations.data ?? [];
  const activeTools = busyTools(operationList);
  const update = useToolUpdateFlow(tools.data, activeTools, actionsBlocked);
  const versions = useToolVersionFlows(tools.data, operationList);
  const awaitingInventory = toolsAwaitingInventory(
    operationList,
    tools.dataUpdatedAt,
  );
  const cardOperations = toolCardOperations(
    operationList,
    dismissedFailures,
    awaitingInventory,
  );
  const submitting =
    install.submitting ||
    update.submitting ||
    uninstall.isPending ||
    versions.submitting;
  const submittingToolIds = new Set<ToolId>(install.submittingTools);
  if (update.submittingTool) submittingToolIds.add(update.submittingTool);
  if (uninstall.isPending && uninstall.variables) {
    submittingToolIds.add(uninstall.variables.tool);
  }
  if (versions.submittingTool) submittingToolIds.add(versions.submittingTool);
  const pendingActions = new Map<ToolId, ToolCardAction>();
  if (versions.submittingTool) {
    pendingActions.set(versions.submittingTool, "version");
  }

  function handleAction(tool: Tool, action: ToolCardAction): void {
    switch (action) {
      case "install":
        install.begin(tool, action);
        return;
      case "update":
        update.begin(tool);
        return;
      case "remove":
        uninstall.reset();
        setRemoving(tool);
        return;
      case "check":
        void tools.refetch();
        return;
      case "open":
        launch.openTool(tool);
        return;
      case "fix":
        install.begin(tool, "repair");
        return;
      case "version":
        versions.beginVersioning(tool);
        return;
    }
  }

  function dismissFailure(operation: Operation): void {
    setDismissedFailures((current) => {
      const next = new Set(current);
      next.add(operation.id);
      return next;
    });
  }

  function retryOperation(tool: Tool, operation: Operation): void {
    const action = retryActionForOperation(operation);
    if (action === null) return;
    dismissFailure(operation);
    handleAction(tool, action);
  }

  function cancelToolOperation(operation: Operation): void {
    if (operation.status !== "running" || !operation.canCancel) return;
    cancelOperation.mutate(operation.id);
  }

  function focusToolById(toolId: ToolId): void {
    const tool = tools.data?.find((candidate) => candidate.id === toolId);
    if (tool) focusToolCard(tool.id);
  }

  function manageDesktopAppConfiguration(app: DesktopApp): void {
    onOpenExtensions?.({ kind: "desktopApp", id: app.id });
  }

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("tools.title")}
      tabIndex={-1}
      className="flex flex-col gap-4 outline-none"
    >
      <ToolsPageInventory
        tools={tools}
        inventoryInitiallyLoading={inventoryInitiallyLoading}
        inventoryUnavailable={inventoryUnavailable}
        inventoryRefreshFailed={inventoryRefreshFailed}
        retryInventory={retryInventory}
      />

      {tools.data && tools.data.length > 0 ? (
        <>
          <TaskAvailabilityNotice
            state={taskAvailability}
            refreshing={operations.isFetching}
            onRetry={() => void operations.refetch()}
          />
          <ToolsGrid
            onDetails={(tool) => setDetailsId(tool.id)}
            tools={tools.data}
            cardOperations={cardOperations}
            awaitingInventory={awaitingInventory}
            actionsBlocked={actionsBlocked}
            checkingVersions={checkingVersions}
            submittingToolIds={submittingToolIds}
            pendingActions={pendingActions}
            launchBusy={launch.busy}
            pendingToolId={launch.pendingToolId}
            onAction={handleAction}
            onDismissOperation={dismissFailure}
            onRetryOperation={retryOperation}
            onRecoverUpdate={versions.beginRecovery}
            cancellingOperationId={
              cancelOperation.isPending ? cancelOperation.variables : null
            }
            onCancelOperation={cancelToolOperation}
          />
        </>
      ) : null}

      <DesktopAppsSection
        className="mt-4"
        onOpenRelatedTool={focusToolById}
        onManageRelatedTool={onOpenServices}
        onManageAppConfiguration={manageDesktopAppConfiguration}
      />

      <ToolsPageDialogs
        pageRef={pageRef}
        actionsBlocked={actionsBlocked}
        install={install}
        installBusy={submitting}
        update={update}
        removing={removing}
        removingBusy={uninstall.isPending}
        removingError={uninstall.error}
        onRemovingOpenChange={(open) => {
          if (open) return;
          uninstall.reset();
          setRemoving(null);
        }}
        onConfirmRemoval={(options) => {
          if (!removing) return;
          uninstall.mutate(
            { tool: removing.id, options },
            { onSuccess: () => setRemoving(null) },
          );
        }}
        versions={versions}
        launch={launch}
      />
      {detailsTool ? (
        <ToolDetailsModal
          key={detailsTool.id}
          tool={detailsTool}
          initialWorkspace={preferredToolId === detailsTool.id}
          onClose={() => setDetailsId(null)}
          onOpenExtensions={onOpenExtensions}
        />
      ) : null}
    </div>
  );
}
