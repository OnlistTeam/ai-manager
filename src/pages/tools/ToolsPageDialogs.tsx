import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import {
  ConfirmActionModal,
  OpenToolModal,
  UpdateConfirmationModal,
  UninstallModal,
  VersionManagementModal,
  type UninstallModalProps,
  useToolLaunchFlow,
} from "@/features/tool-management";
import type { ToolInstallFlow } from "./useToolInstallFlow";
import type { ToolUpdateFlow } from "./useToolUpdateFlow";
import type { ToolVersionFlows } from "./useToolVersionFlows";

export interface ToolsPageDialogsProps {
  pageRef: RefObject<HTMLDivElement>;
  /** Inventory or task list is unreliable: nothing may be confirmed. */
  actionsBlocked: boolean;
  install: ToolInstallFlow;
  /** Any lifecycle write is in flight; the install dialog waits for all of them. */
  installBusy: boolean;
  update: ToolUpdateFlow;
  removing: Tool | null;
  removingBusy: boolean;
  removingError: Error | null;
  onRemovingOpenChange: (open: boolean) => void;
  onConfirmRemoval: UninstallModalProps["onConfirm"];
  versions: ToolVersionFlows;
  launch: ReturnType<typeof useToolLaunchFlow>;
}

/** Every dialog the Software page can open; the page itself only dispatches actions. */
export function ToolsPageDialogs({
  pageRef,
  actionsBlocked,
  install,
  installBusy,
  update,
  removing,
  removingBusy,
  removingError,
  onRemovingOpenChange,
  onConfirmRemoval,
  versions,
  launch,
}: ToolsPageDialogsProps) {
  const { t } = useTranslation();

  return (
    <>
      <ConfirmActionModal
        open={install.pending !== null}
        onOpenChange={install.setOpen}
        title={t(
          `tools.confirm.${install.pending?.action ?? "install"}.title`,
          { name: install.pending?.tool.name ?? "" },
        )}
        description={t(
          `tools.confirm.${install.pending?.action ?? "install"}.body`,
        )}
        confirmLabel={t(
          `tools.confirm.${install.pending?.action ?? "install"}.confirm`,
        )}
        busy={installBusy}
        error={install.error}
        onConfirm={install.confirm}
      />

      <UpdateConfirmationModal
        open={update.open}
        tools={update.tool ? [update.tool] : []}
        previews={update.previews.data}
        loading={update.previews.isPending}
        refreshing={
          update.previews.isFetching && update.previews.data !== undefined
        }
        previewError={update.previews.error}
        submitting={update.submitting}
        mutationError={update.error}
        returnFocusFallbackRef={pageRef}
        actionPaused={update.paused}
        onOpenChange={update.setOpen}
        onRefresh={update.refresh}
        onConfirm={update.confirm}
      />

      <UninstallModal
        tool={removing}
        busy={removingBusy}
        error={removingError}
        onOpenChange={onRemovingOpenChange}
        onConfirm={onConfirmRemoval}
      />

      <VersionManagementModal
        tool={versions.versioning}
        busy={versions.submitting}
        error={versions.versioningError}
        onOpenChange={versions.setVersioningOpen}
        onConfirm={versions.confirmVersion}
      />

      <ConfirmActionModal
        open={versions.recovering !== null}
        onOpenChange={versions.setRecoveryOpen}
        title={t("tools.updateRecovery.confirmTitle", {
          name: versions.recovering?.tool.name ?? "",
          version: versions.recovering?.version ?? "",
        })}
        description={t("tools.updateRecovery.confirmBody")}
        confirmLabel={t("tools.updateRecovery.confirm", {
          version: versions.recovering?.version ?? "",
        })}
        busy={versions.submitting}
        confirmDisabled={!versions.recoveryValid || actionsBlocked}
        error={versions.recoveryError}
        onConfirm={versions.confirmRecovery}
      />

      <OpenToolModal
        tool={launch.tool}
        busy={launch.busy}
        error={launch.error}
        providerRecovery={launch.providerRecovery}
        onOpenChange={launch.onOpenChange}
        onLaunchDefault={() => launch.confirm("default")}
        onChooseFolder={() => launch.confirm("choose")}
      />
    </>
  );
}
