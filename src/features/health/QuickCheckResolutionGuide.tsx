import { ArrowRight } from "lucide-react";
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
import { Button } from "@/shared/ui/Button";
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

function isIssue(item: QuickCheckItem): boolean {
  return item.status === "attention" || item.status === "action";
}

const RESOLUTION_PRIORITY: Record<QuickCheckResolution, number> = {
  repairTool: 0,
  reviewTool: 1,
  connectService: 2,
  reviewService: 3,
  updateTool: 4,
  reviewExtensions: 5,
};

function rank(item: QuickCheckItem): number {
  const severity = item.status === "action" ? 0 : 100;
  const priority =
    item.resolution === undefined ? 99 : RESOLUTION_PRIORITY[item.resolution];
  return severity + priority;
}

/** The first problem gets an explicit, confirmed step; the rest stay folded. */
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
  const issues = summary.items
    .filter(isIssue)
    .sort((left, right) => rank(left) - rank(right));

  if (issues.length === 0) return null;

  const current = issues[0];
  const remaining = issues.slice(1);
  const tool = tools.find((candidate) => candidate.id === current.toolId);
  const directResolution =
    current.resolution !== undefined &&
    isDirect(current.resolution) &&
    tool !== undefined
      ? current.resolution
      : null;
  const active =
    current.toolId !== null && activeTools.has(current.toolId as ToolId);
  const directBlocked =
    directResolution !== null && (tasks.actionsBlocked || active);
  const confirmationActive = confirmation
    ? activeTools.has(confirmation.tool.id)
    : false;
  const updateActive = updateToolId !== null && activeTools.has(updateToolId);
  const effectiveAction =
    current.resolution === undefined
      ? null
      : (directResolution ??
        (current.resolution === "updateTool" ||
        current.resolution === "repairTool" ||
        current.resolution === "reviewTool"
          ? "reviewTool"
          : current.resolution));

  function closeConfirmation(): void {
    if (update.isPending || repair.isPending) return;
    update.reset();
    repair.reset();
    setConfirmation(null);
  }

  function continueCurrent(): void {
    if (directResolution && tool) {
      update.reset();
      repair.reset();
      setConfirmation({ resolution: directResolution, tool });
      return;
    }
    switch (effectiveAction) {
      case "connectService":
      case "reviewService":
        onOpenServices?.(current.toolId ?? undefined);
        return;
      case "reviewExtensions":
        onOpenExtensions?.();
        return;
      case "reviewTool":
      case "updateTool":
      case "repairTool":
        onOpenTools?.();
        return;
      case null:
        return;
    }
  }

  return (
    <>
      <TaskAvailabilityNotice
        state={tasks.operations.isError ? "unavailable" : tasks.state}
        refreshing={tasks.operations.isFetching}
        onRetry={() => void tasks.operations.refetch()}
      />
      <div ref={guideRef} tabIndex={-1} className="flex flex-col gap-3">
        <div className="flex flex-col gap-3 rounded-lg border border-hairline bg-layer-1 p-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="min-w-0">
            <p className="text-body font-medium text-content">
              {t(current.titleKey, current.values)}
            </p>
            <p className="mt-1 text-caption leading-5 text-content-muted">
              {t(current.descriptionKey, current.values)}
            </p>
          </div>
          {effectiveAction ? (
            <Button
              className="shrink-0"
              disabled={directBlocked}
              onClick={continueCurrent}
            >
              {t(`preferences.check.guide.action.${effectiveAction}`)}
              <ArrowRight className="h-4 w-4" aria-hidden="true" />
            </Button>
          ) : null}
        </div>

        {remaining.length > 0 ? (
          <details className="rounded-lg border border-hairline bg-layer-1 px-3 py-2">
            <summary className="cursor-pointer list-none text-caption text-content-muted">
              {t("home.health.more", { count: remaining.length })}
            </summary>
            <ul className="mt-3 grid gap-2 border-t border-hairline pt-3">
              {remaining.map((item) => (
                <QuickCheckSignalItem key={item.id} item={item} />
              ))}
            </ul>
          </details>
        ) : null}
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
