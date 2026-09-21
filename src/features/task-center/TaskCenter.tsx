import { useId, useRef, useState } from "react";
import * as PopoverPrimitive from "@radix-ui/react-popover";
import { History, TriangleAlert, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  sortByRecency,
  type Operation,
  useOperations,
} from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import {
  busyTools,
  isToolLaunchable,
  useCancelOperation,
  useToolLaunchFlow,
} from "@/features/tool-management";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { cn } from "@/shared/ui/cn";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { TaskCenterDialogs } from "./TaskCenterDialogs";
import type { TaskCenterDetails } from "./TaskCenterOperationItem";
import { TaskCenterPanelBody } from "./TaskCenterPanelBody";
import { countActive } from "./operationPresentation";
import { useTaskCenterToolRecovery } from "./useTaskCenterToolRecovery";
import { useTaskCenterUpdateRecovery } from "./useTaskCenterUpdateRecovery";
import { useTaskRetryFocus } from "./useTaskRetryFocus";

/** Spec §40: a unified task center in the top-right corner that keeps recent history after tasks finish. */
export function TaskCenter() {
  const { t } = useTranslation();
  const operations = useOperations();
  const launch = useToolLaunchFlow();
  const cancelOperation = useCancelOperation();
  const [open, setOpen] = useState(false);
  const [details, setDetails] = useState<TaskCenterDetails | null>(null);
  const titleId = useId();
  const statusId = useId();
  const titleRef = useRef<HTMLHeadingElement>(null);
  const { retryButtonRef, retryTasks } = useTaskRetryFocus(
    operations,
    titleRef,
  );
  const {
    tools,
    retryButtonRef: toolRetryButtonRef,
    unavailable: toolsUnavailable,
    refreshFailed: toolsRefreshFailed,
    actionsBlocked: toolActionsBlocked,
    retryTools,
  } = useTaskCenterToolRecovery(titleRef);

  const list = sortByRecency(operations.data ?? []);
  const busyToolIds = busyTools(list);
  const active = countActive(list);
  const toolOf = (id: Operation["tool"]): Tool | null =>
    tools.data?.find((tool) => tool.id === id) ?? null;
  const recovery = useTaskCenterUpdateRecovery(
    list,
    toolOf,
    busyToolIds,
    toolActionsBlocked,
  );
  const currentLaunchTool = launch.tool ? toolOf(launch.tool.id) : null;
  const launchToolChanged = Boolean(
    launch.tool &&
      tools.isSuccess &&
      (!currentLaunchTool || !isToolLaunchable(currentLaunchTool)),
  );
  const launchActionsBlocked = toolActionsBlocked || launchToolChanged;
  const launchPauseReason = launchToolChanged
    ? "changed"
    : tools.isError
      ? "failed"
      : "checking";
  const nestedDialogOpen =
    details !== null || launch.tool !== null || recovery.request !== null;
  const unavailable = operations.isError && !operations.data;
  const refreshFailed = operations.isError && Boolean(operations.data);
  const status = operations.isPending
    ? t("taskCenter.loading.summary")
    : unavailable
      ? t("taskCenter.unavailable.summary")
      : refreshFailed
        ? t("taskCenter.refreshError.summary")
        : active > 0
          ? t("taskCenter.running", { count: active })
          : t("taskCenter.idle");
  const triggerLabel = unavailable
    ? t("taskCenter.openUnavailable")
    : refreshFailed
      ? t("taskCenter.openRefreshError")
      : tools.isError
        ? t("taskCenter.toolAuthority.openLabel", { status })
        : active > 0
          ? t("taskCenter.openRunning", { count: active })
          : t("taskCenter.open");
  const shouldShowTrigger =
    operations.isPending ||
    unavailable ||
    refreshFailed ||
    tools.isError ||
    list.length > 0;

  const openTool = (tool: Tool): void => {
    if (!toolActionsBlocked) launch.openTool(tool);
  };

  const cancelTask = (operation: Operation): void => {
    if (
      operation.status !== "running" ||
      !operation.canCancel ||
      cancelOperation.isPending
    ) {
      return;
    }
    cancelOperation.mutate(operation.id);
  };

  if (!shouldShowTrigger) return null;

  return (
    <>
      <PopoverPrimitive.Root
        open={open}
        onOpenChange={(nextOpen) => {
          if (!nextOpen && nestedDialogOpen) return;
          setOpen(nextOpen);
        }}
      >
        <PopoverPrimitive.Trigger asChild>
          <Button variant="ghost" size="sm" aria-label={triggerLabel}>
            <History className="h-4 w-4" aria-hidden="true" />
            {t("taskCenter.open")}
            {active > 0 ? (
              <Badge tone="brand" aria-hidden="true">
                {active}
              </Badge>
            ) : null}
            {unavailable || refreshFailed || tools.isError ? (
              <span
                data-task-warning=""
                aria-hidden="true"
                className="flex h-5 w-5 shrink-0 items-center justify-center rounded-full bg-warning/10 text-warning"
              >
                <TriangleAlert className="h-3.5 w-3.5" />
              </span>
            ) : null}
          </Button>
        </PopoverPrimitive.Trigger>
        <PopoverPrimitive.Portal>
          <PopoverPrimitive.Content
            role="dialog"
            aria-labelledby={titleId}
            aria-describedby={statusId}
            align="end"
            sideOffset={8}
            collisionPadding={16}
            onOpenAutoFocus={(event) => {
              event.preventDefault();
              titleRef.current?.focus();
            }}
            onEscapeKeyDown={(event) => {
              if (nestedDialogOpen) event.preventDefault();
            }}
            onInteractOutside={(event) => {
              if (nestedDialogOpen) event.preventDefault();
            }}
            // Spec §48 names the floating task centre as one of the three glass
            // surfaces. A lift layer is not enough here: the panel hangs over
            // whatever the route was showing, so it needs the opaque glass base
            // to stay readable rather than letting cards bleed through.
            className="app-floating-surface z-50 flex max-h-[min(38rem,var(--radix-popover-content-available-height))] w-[min(26rem,calc(100vw-2rem))] flex-col overflow-hidden rounded-xl border animate-ds-modal-in"
          >
            <div className="flex items-start justify-between gap-4 border-b border-hairline px-4 py-3.5">
              <div className="min-w-0">
                <h2
                  ref={titleRef}
                  id={titleId}
                  tabIndex={-1}
                  className={cn(
                    "w-fit rounded-sm text-heading text-content",
                    FOCUS_RING,
                  )}
                >
                  {t("taskCenter.title")}
                </h2>
                <p
                  id={statusId}
                  className="mt-0.5 text-caption text-content-muted"
                >
                  {status}
                </p>
              </div>
              <PopoverPrimitive.Close asChild>
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-8 w-8 shrink-0 p-0"
                  aria-label={t("ds.action.close")}
                >
                  <X className="h-4 w-4" aria-hidden="true" />
                </Button>
              </PopoverPrimitive.Close>
            </div>

            <TaskCenterPanelBody
              operations={list}
              pending={operations.isPending}
              unavailable={unavailable}
              refreshFailed={refreshFailed}
              refreshing={operations.isFetching}
              toolsUnavailable={toolsUnavailable}
              toolsRefreshFailed={toolsRefreshFailed}
              toolsRefreshing={tools.isFetching}
              toolActionsBlocked={toolActionsBlocked}
              busyToolIds={busyToolIds}
              activeHeadingId={`${titleId}-active`}
              recentHeadingId={`${titleId}-recent`}
              toolOf={toolOf}
              retryButtonRef={retryButtonRef}
              toolRetryButtonRef={toolRetryButtonRef}
              onRetry={retryTasks}
              onRetryTools={retryTools}
              onOpenTool={openTool}
              cancellingOperationId={
                cancelOperation.isPending ? cancelOperation.variables : null
              }
              onCancel={cancelTask}
              onRecoverUpdate={recovery.begin}
              onViewDetails={setDetails}
            />
          </PopoverPrimitive.Content>
        </PopoverPrimitive.Portal>
      </PopoverPrimitive.Root>

      <TaskCenterDialogs
        details={details}
        onDetailsOpenChange={(nextOpen) => {
          if (!nextOpen) setDetails(null);
        }}
        recovery={recovery}
        toolActionsBlocked={toolActionsBlocked}
        launch={launch}
        launchActionsBlocked={launchActionsBlocked}
        launchPauseReason={launchPauseReason}
        titleRef={titleRef}
      />
    </>
  );
}
