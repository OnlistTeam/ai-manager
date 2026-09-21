import type { RefObject } from "react";
import { useTranslation } from "react-i18next";
import {
  ConfirmActionModal,
  OpenToolModal,
  useToolLaunchFlow,
} from "@/features/tool-management";
import { ErrorDetailsModal } from "./ErrorDetailsModal";
import type { TaskCenterDetails } from "./TaskCenterOperationItem";
import { TaskToolActionsPausedNotice } from "./TaskToolAuthorityNotice";
import type { TaskCenterUpdateRecovery } from "./useTaskCenterUpdateRecovery";

export interface TaskCenterDialogsProps {
  details: TaskCenterDetails | null;
  onDetailsOpenChange: (open: boolean) => void;
  recovery: TaskCenterUpdateRecovery;
  /** The tool inventory is unreliable: no restore may be confirmed. */
  toolActionsBlocked: boolean;
  launch: ReturnType<typeof useToolLaunchFlow>;
  /** Tool inventory unreliable, or the tool being opened changed underneath. */
  launchActionsBlocked: boolean;
  launchPauseReason: "changed" | "failed" | "checking";
  titleRef: RefObject<HTMLHeadingElement>;
}

/** The dialogs the task centre opens on top of its popover. */
export function TaskCenterDialogs({
  details,
  onDetailsOpenChange,
  recovery,
  toolActionsBlocked,
  launch,
  launchActionsBlocked,
  launchPauseReason,
  titleRef,
}: TaskCenterDialogsProps) {
  const { t } = useTranslation();

  return (
    <>
      <ErrorDetailsModal
        error={details?.error ?? null}
        taskName={details?.taskName ?? null}
        onOpenChange={onDetailsOpenChange}
      />

      <ConfirmActionModal
        open={recovery.request !== null}
        onOpenChange={recovery.setOpen}
        title={t("tools.updateRecovery.confirmTitle", {
          name: recovery.request?.toolName ?? "",
          version: recovery.request?.version ?? "",
        })}
        description={t("tools.updateRecovery.confirmBody")}
        confirmLabel={t("tools.updateRecovery.confirm", {
          version: recovery.request?.version ?? "",
        })}
        busy={recovery.busy}
        confirmDisabled={!recovery.valid || toolActionsBlocked}
        error={recovery.error}
        onConfirm={recovery.confirm}
      />

      <OpenToolModal
        tool={launch.tool}
        busy={launch.busy}
        confirmDisabled={launchActionsBlocked}
        error={launch.error}
        providerRecovery={launch.providerRecovery}
        notice={
          launchActionsBlocked && !launch.busy ? (
            <TaskToolActionsPausedNotice reason={launchPauseReason} />
          ) : null
        }
        returnFocusFallbackRef={titleRef}
        onOpenChange={launch.onOpenChange}
        onLaunchDefault={() => {
          if (!launchActionsBlocked) launch.confirm("default");
        }}
        onChooseFolder={() => {
          if (!launchActionsBlocked) launch.confirm("choose");
        }}
      />
    </>
  );
}
