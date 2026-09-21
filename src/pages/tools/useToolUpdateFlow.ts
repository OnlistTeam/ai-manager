import { useRef, useState } from "react";
import {
  useToolUpdatePreviews,
  type Tool,
  type ToolId,
  type ToolUpdateReadyPreview,
} from "@/entities/tool";
import {
  useLifecycleErrorNotice,
  useUpdateTool,
} from "@/features/tool-management";

export interface ToolUpdateFlow {
  /** The tool being confirmed, re-read from the fresh inventory; null when closed. */
  tool: Tool | null;
  open: boolean;
  previews: ReturnType<typeof useToolUpdatePreviews>;
  submitting: boolean;
  /** Which tool the in-flight update belongs to, for the card's busy state. */
  submittingTool: ToolId | null;
  /** Shown inside the dialog only; once it is dismissed failures go to a notice. */
  error: Error | null;
  /** Confirmation is locked: inventory or tasks are unreliable, the tool changed, or it is already busy. */
  paused: boolean;
  begin: (tool: Tool) => void;
  setOpen: (open: boolean) => void;
  refresh: () => void;
  confirm: (ready: readonly ToolUpdateReadyPreview[]) => void;
}

/**
 * One tool's update confirmation: the dialog state, its preview read and the
 * hand-off itself. Authorisation is re-derived from the fresh inventory on
 * every render, so a tool that changed underneath the dialog locks it.
 */
export function useToolUpdateFlow(
  tools: readonly Tool[] | undefined,
  busyToolIds: ReadonlySet<ToolId>,
  actionsBlocked: boolean,
): ToolUpdateFlow {
  const update = useUpdateTool({ notifyOnError: false });
  const notifyLifecycleError = useLifecycleErrorNotice();
  const [updatingId, setUpdatingId] = useState<ToolId | null>(null);
  /** Whether the user closed the confirmation dialog before this hand-off finished (decides whether a failure result surfaces in-page or as a notification). */
  const handoffDismissed = useRef(false);
  const updating = tools?.find((tool) => tool.id === updatingId) ?? null;
  const stillAuthorized = Boolean(
    updating &&
      updating.status === "updateAvailable" &&
      updating.capabilities.canUpdate,
  );
  const previews = useToolUpdatePreviews(
    updatingId ? [updatingId] : [],
    updatingId !== null,
  );
  const active = updatingId !== null && busyToolIds.has(updatingId);

  return {
    tool: updating,
    open: updatingId !== null,
    previews,
    submitting: update.isPending,
    submittingTool:
      update.isPending && update.variables ? update.variables.tool : null,
    error: updating ? update.error : null,
    paused: actionsBlocked || !stillAuthorized || active,
    begin: (tool) => {
      update.reset();
      setUpdatingId(tool.id);
    },
    setOpen: (open) => {
      if (open) return;
      // Let the user go even if the hand-off hasn't finished: it will still
      // run to completion, and the task will show up in "Activity". We can't
      // `reset()` here — that would also discard this hand-off's result.
      if (update.isPending) handoffDismissed.current = true;
      else update.reset();
      setUpdatingId(null);
    },
    refresh: () => {
      update.reset();
      void previews.refetch();
    },
    confirm: (ready) => {
      if (!updatingId || !stillAuthorized || active || actionsBlocked) return;
      const preview = ready.find((item) => item.tool === updatingId);
      if (!preview) return;
      handoffDismissed.current = false;
      update.mutate(
        {
          tool: updatingId,
          previewFingerprint: preview.previewFingerprint,
        },
        {
          onSuccess: () => setUpdatingId(null),
          // While the dialog is still open, the error shows inside it
          // (alongside "Check again"); once the user has navigated away,
          // that area is off-screen and only a transient notification can
          // deliver the result.
          onError: (error) => {
            if (handoffDismissed.current) notifyLifecycleError(error);
          },
        },
      );
    },
  };
}
