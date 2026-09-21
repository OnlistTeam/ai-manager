import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  activeToolOperations,
  UpdateConfirmationModal,
  useRepairTool,
  useUpdateTool,
} from "@/features/tool-management";
import {
  TaskAvailabilityNotice,
  useTaskAvailability,
} from "@/features/task-center";
import { type Tool, type ToolId, useToolUpdatePreviews } from "@/entities/tool";
import type {
  QuickCheckItem,
  QuickCheckResolution,
  QuickCheckSummary,
} from "./quickCheck";
import { QuickCheckSignalItem } from "./QuickCheckItem";
import {
  QuickCheckResolutionModal,
  type DirectQuickCheckResolution,
  type QuickCheckConfirmation,
} from "./QuickCheckResolutionModal";

export interface QuickCheckResolutionGuideProps {
  summary: QuickCheckSummary;
  tools: readonly Tool[];
  onOpenTools?: () => void;
  onOpenServices?: (toolId?: ToolId) => void;
  onOpenExtensions?: () => void;
}

function isDirect(
  resolution: QuickCheckResolution,
): resolution is DirectQuickCheckResolution {
  return ["updateTool", "repairTool"].includes(resolution);
}

/**
 * A finding is something with a next step.
 *
 * Severity alone is the wrong filter in both directions. "Pi is not installed",
 * "this service was not checked this run" and "2 MCP servers" are true, have no
 * action, and belong to an inventory rather than to a health check — listing
 * them buries the two rows that matter under ten that do not. Meanwhile "no AI
 * service configured" is only informational yet does have an offer worth
 * making, so it stays.
 */
function isFinding(item: QuickCheckItem): boolean {
  return item.status !== "ready" && item.resolution !== undefined;
}

const RESOLUTION_PRIORITY: Record<QuickCheckResolution, number> = {
  repairTool: 0,
  reviewTool: 1,
  connectService: 2,
  reviewService: 3,
  updateTool: 4,
  reviewExtensions: 5,
};

const SEVERITY: Record<QuickCheckItem["status"], number> = {
  action: 0,
  attention: 10,
  info: 20,
  ready: 30,
};

function rank(item: QuickCheckItem): number {
  const priority =
    item.resolution === undefined ? 9 : RESOLUTION_PRIORITY[item.resolution];
  return SEVERITY[item.status] + priority;
}

/**
 * Every finding as a peer row, most urgent first.
 *
 * This used to promote the first problem into an expanded card and fold the
 * rest behind "5 more", which meant the page named one thing and hid the
 * shape of everything else. Flat rows are narrow enough that the whole list
 * fits, and each row carries its own next step, so nothing has to be unfolded
 * before it can be acted on.
 */
export function QuickCheckResolutionGuide({
  summary,
  tools,
  onOpenTools,
  onOpenServices,
  onOpenExtensions,
}: QuickCheckResolutionGuideProps) {
  const { t } = useTranslation();
  const guideRef = useRef<HTMLDivElement>(null);
  const [confirmation, setConfirmation] =
    useState<QuickCheckConfirmation | null>(null);
  const update = useUpdateTool({ notifyOnError: false });
  const repair = useRepairTool({ notifyOnError: false });
  const updateToolId =
    confirmation?.resolution === "updateTool" ? confirmation.tool.id : null;
  const updateTool =
    tools.find((candidate) => candidate.id === updateToolId) ?? null;
  const updateStillAuthorized = Boolean(
    updateTool &&
      updateTool.status === "updateAvailable" &&
      updateTool.capabilities.canUpdate,
  );
  const repairConfirmation =
    confirmation?.resolution === "repairTool" ? confirmation : null;
  const updatePreviews = useToolUpdatePreviews(
    updateTool ? [updateTool.id] : [],
    updateTool !== null,
  );
  const tasks = useTaskAvailability();
  const activeTools = activeToolOperations(tasks.operations.data ?? []);
  const listed = summary.items
    .filter(isFinding)
    .sort((left, right) => rank(left) - rank(right));

  if (listed.length === 0) return null;

  function closeConfirmation(): void {
    if (update.isPending || repair.isPending) return;
    update.reset();
    repair.reset();
    setConfirmation(null);
  }

  /**
   * The row's own step. An update or a repair is a real change, so it opens the
   * same confirmation the tools page uses; everything else navigates to the
   * page that owns the fix.
   */
  function actionFor(item: QuickCheckItem) {
    if (item.resolution === undefined) return undefined;
    const tool = tools.find((candidate) => candidate.id === item.toolId);
    const direct =
      isDirect(item.resolution) && tool !== undefined ? item.resolution : null;
    const active =
      item.toolId !== null && activeTools.has(item.toolId as ToolId);
    const effective =
      direct ??
      (item.resolution === "updateTool" ||
      item.resolution === "repairTool" ||
      item.resolution === "reviewTool"
        ? "reviewTool"
        : item.resolution);

    return {
      label: t(`preferences.check.guide.action.${effective}`),
      disabled: direct !== null && (tasks.actionsBlocked || active),
      onSelect: () => {
        if (direct && tool) {
          update.reset();
          repair.reset();
          setConfirmation({ resolution: direct, tool });
          return;
        }
        switch (effective) {
          case "connectService":
          case "reviewService":
            onOpenServices?.(item.toolId ?? undefined);
            return;
          case "reviewExtensions":
            onOpenExtensions?.();
            return;
          case "reviewTool":
          case "updateTool":
          case "repairTool":
            onOpenTools?.();
            return;
        }
      },
    };
  }

  const confirmationActive = confirmation
    ? activeTools.has(confirmation.tool.id)
    : false;
  const updateActive = updateToolId !== null && activeTools.has(updateToolId);

  return (
    <>
      <TaskAvailabilityNotice
        state={tasks.operations.isError ? "unavailable" : tasks.state}
        refreshing={tasks.operations.isFetching}
        onRetry={() => void tasks.operations.refetch()}
      />
      <div ref={guideRef} tabIndex={-1} className="flex flex-col gap-3">
        <ul className="grid gap-2">
          {listed.map((item) => (
            <QuickCheckSignalItem
              key={item.id}
              item={item}
              action={actionFor(item)}
            />
          ))}
        </ul>
      </div>

      <QuickCheckResolutionModal
        confirmation={repairConfirmation}
        busy={repair.isPending}
        disabled={tasks.actionsBlocked || confirmationActive}
        error={repair.error}
        onClose={closeConfirmation}
        onConfirm={() => {
          if (!repairConfirmation || tasks.actionsBlocked || confirmationActive)
            return;
          repair.mutate(repairConfirmation.tool.id, {
            onSuccess: () => setConfirmation(null),
          });
        }}
      />

      <UpdateConfirmationModal
        open={updateTool !== null}
        tools={updateTool ? [updateTool] : []}
        previews={updatePreviews.data}
        loading={updatePreviews.isPending}
        refreshing={
          updatePreviews.isFetching && updatePreviews.data !== undefined
        }
        previewError={updatePreviews.error}
        submitting={update.isPending}
        mutationError={update.error}
        returnFocusFallbackRef={guideRef}
        actionPaused={
          tasks.actionsBlocked ||
          confirmationActive ||
          !updateStillAuthorized ||
          updateActive
        }
        onOpenChange={(open) => {
          if (!open) closeConfirmation();
        }}
        onRefresh={() => {
          update.reset();
          void updatePreviews.refetch();
        }}
        onConfirm={(ready) => {
          if (
            !updateTool ||
            !updateStillAuthorized ||
            updateActive ||
            tasks.actionsBlocked ||
            confirmationActive
          )
            return;
          const preview = ready.find((item) => item.tool === updateTool.id);
          if (!preview) return;
          update.mutate(
            {
              tool: updateTool.id,
              previewFingerprint: preview.previewFingerprint,
            },
            { onSuccess: () => setConfirmation(null) },
          );
        }}
      />
    </>
  );
}
