import { useRef, useState } from "react";
import {
  useToolUpdatePreviews,
  type Tool,
  type ToolUpdateReadyPreview,
} from "@/entities/tool";
import {
  useLifecycleErrorNotice,
  useUpdateTool,
} from "@/features/tool-management";

export interface UpdateAttemptFailure {
  failedCount: number;
  startedCount: number;
}

export interface HomeUpdateAll {
  /** Tools in the confirmation dialog; empty when it is closed. */
  pending: readonly Tool[];
  previews: ReturnType<typeof useToolUpdatePreviews>;
  /** The hand-offs are being queued one by one. */
  scheduling: boolean;
  failure: UpdateAttemptFailure | null;
  /** Tools that were dropped from the batch because they became busy or changed. */
  skippedCount: number;
  /** Every tool in the dialog is still ready according to the fresh inventory. */
  stillAuthorized: boolean;
  start: () => void;
  setOpen: (open: boolean) => void;
  refresh: () => void;
  confirm: (ready: readonly ToolUpdateReadyPreview[]) => void;
}

interface HomeUpdateAllInputs {
  readyToUpdate: readonly Tool[];
  actionsBlocked: boolean;
  refetchOperations: () => Promise<unknown>;
}

/**
 * The Update All batch: one confirmation for every ready tool, handed off one
 * by one. §41: a Tool that already has a task isn't re-enqueued; other Tools
 * can still update concurrently.
 */
export function useHomeUpdateAll({
  readyToUpdate,
  actionsBlocked,
  refetchOperations,
}: HomeUpdateAllInputs): HomeUpdateAll {
  const update = useUpdateTool({ notifyOnError: false });
  const notifyLifecycleError = useLifecycleErrorNotice();
  const [pendingUpdates, setPendingUpdates] = useState<readonly Tool[]>([]);
  const [previewRevision, setPreviewRevision] = useState(0);
  const updatePreviews = useToolUpdatePreviews(
    pendingUpdates.map((tool) => tool.id),
    pendingUpdates.length > 0,
    previewRevision,
  );
  const [schedulingUpdates, setSchedulingUpdates] = useState(false);
  /** Whether the user closed the confirmation dialog before this batch of handoffs finished queuing (decides whether failure results surface in-page or via notification). */
  const handoffDismissed = useRef(false);
  const [updateFailure, setUpdateFailure] =
    useState<UpdateAttemptFailure | null>(null);
  const [skippedCount, setSkippedCount] = useState(0);
  const readyUpdateIds = new Set(readyToUpdate.map((tool) => tool.id));
  const pendingStillAuthorized = pendingUpdates.every((tool) =>
    readyUpdateIds.has(tool.id),
  );

  async function beginReadyUpdates(
    candidates: readonly ToolUpdateReadyPreview[],
  ): Promise<void> {
    if (
      candidates.length === 0 ||
      schedulingUpdates ||
      actionsBlocked ||
      !pendingStillAuthorized
    )
      return;

    handoffDismissed.current = false;
    setSchedulingUpdates(true);
    try {
      const results = await Promise.allSettled(
        candidates.map((preview) =>
          update.mutateAsync({
            tool: preview.tool,
            previewFingerprint: preview.previewFingerprint,
          }),
        ),
      );
      await refetchOperations();
      const failures = candidates.flatMap((preview, index) => {
        const result = results[index];
        return result?.status === "rejected"
          ? [{ tool: preview.tool, error: result.reason as Error }]
          : [];
      });
      const failedIds = new Set(failures.map((failure) => failure.tool));
      const failed = pendingUpdates.filter((tool) => failedIds.has(tool.id));
      const newlySkipped = pendingUpdates.length - candidates.length;
      setSkippedCount((current) => current + newlySkipped);
      if (failed.length > 0) {
        update.reset();
        if (handoffDismissed.current) {
          // The user has already left this dialog. Popping it back up to
          // steal focus would be worse than not reporting at all, so use
          // transient notifications delivered one by one instead — exactly
          // the channel that `notifyOnError: false` freed up.
          for (const failure of failures) notifyLifecycleError(failure.error);
          return;
        }
        setPendingUpdates(failed);
        setPreviewRevision((current) => current + 1);
        setUpdateFailure({
          failedCount: failed.length,
          startedCount: candidates.length - failed.length,
        });
        return;
      }
      update.reset();
      setPendingUpdates([]);
      setUpdateFailure(null);
    } finally {
      setSchedulingUpdates(false);
    }
  }

  return {
    pending: pendingUpdates,
    previews: updatePreviews,
    scheduling: schedulingUpdates,
    failure: updateFailure,
    skippedCount,
    stillAuthorized: pendingStillAuthorized,
    start: () => {
      update.reset();
      setUpdateFailure(null);
      setSkippedCount(0);
      setPreviewRevision((current) => current + 1);
      setPendingUpdates(readyToUpdate);
    },
    setOpen: (open) => {
      if (open) return;
      // Let the user go even if this batch of handoffs hasn't finished
      // queuing: the remaining tasks still get queued into "Activity" as
      // usual. Must not `reset()` here — that would throw away this batch's
      // results along with it.
      if (schedulingUpdates) handoffDismissed.current = true;
      else update.reset();
      setUpdateFailure(null);
      setSkippedCount(0);
      setPendingUpdates([]);
    },
    refresh: () => {
      update.reset();
      void updatePreviews.refetch();
    },
    confirm: (ready) => void beginReadyUpdates(ready),
  };
}
