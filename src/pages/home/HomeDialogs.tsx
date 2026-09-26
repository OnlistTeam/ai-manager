import type { RefObject } from "react";
import {
  OpenToolModal,
  UpdateConfirmationModal,
  type useToolLaunchFlow,
} from "@/features/tool-management";
import {
  HomeActionCheckingNotice,
  HomeActionPausedNotice,
} from "./HomeReadinessNotice";
import { UpdateAllFailureNotice } from "./UpdateAllFailureNotice";
import type { HomeUpdateAll } from "./useHomeUpdateAll";

export interface HomeDialogsProps {
  updates: HomeUpdateAll;
  launch: ReturnType<typeof useToolLaunchFlow>;
  pageRef: RefObject<HTMLElement>;
  actionsBlocked: boolean;
  retrying: boolean;
  refreshFailed: boolean;
  onReviewSkipped: () => void;
}

/**
 * The two confirmations Home can open: the update review (for every ready
 * tool, or for one row) and Start. Both pause while Home's read authorities
 * are refreshing or failed, and say why.
 */
export function HomeDialogs({
  updates,
  launch,
  pageRef,
  actionsBlocked,
  retrying,
  refreshFailed,
  onReviewSkipped,
}: HomeDialogsProps) {
  const readiness =
    retrying && !refreshFailed ? (
      <HomeActionCheckingNotice />
    ) : refreshFailed ? (
      <HomeActionPausedNotice />
    ) : null;

  return (
    <>
      <UpdateConfirmationModal
        open={updates.pending.length > 0}
        tools={updates.pending}
        previews={updates.previews.data}
        loading={updates.previews.isPending}
        refreshing={
          updates.previews.isFetching && updates.previews.data !== undefined
        }
        previewError={updates.previews.error}
        submitting={updates.scheduling}
        mutationError={null}
        returnFocusFallbackRef={pageRef}
        actionPaused={actionsBlocked || !updates.stillAuthorized}
        bulk={updates.bulk}
        notice={
          <>
            {readiness}
            {updates.failure ? (
              <UpdateAllFailureNotice
                {...updates.failure}
                skippedCount={updates.skippedCount}
                onReviewSkipped={onReviewSkipped}
              />
            ) : null}
          </>
        }
        onOpenChange={updates.setOpen}
        onRefresh={updates.refresh}
        onConfirm={updates.confirm}
      />

      <OpenToolModal
        tool={launch.tool}
        busy={launch.busy}
        confirmDisabled={actionsBlocked}
        error={launch.error}
        providerRecovery={launch.providerRecovery}
        notice={readiness}
        onOpenChange={launch.onOpenChange}
        onLaunchDefault={() => {
          if (!actionsBlocked) launch.confirm("default");
        }}
        onChooseFolder={() => {
          if (!actionsBlocked) launch.confirm("choose");
        }}
      />
    </>
  );
}
