import { ClipboardList, RefreshCw, TriangleAlert } from "lucide-react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import { isTerminal, type Operation } from "@/entities/operation";
import type { Tool, ToolId } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { DetectionStatus } from "@/shared/ui/DetectionStatus";
import { EmptyState } from "@/shared/ui/EmptyState";
import {
  TaskCenterOperationItem,
  type TaskCenterDetails,
} from "./TaskCenterOperationItem";
import { TaskToolInventoryNotice } from "./TaskToolAuthorityNotice";

interface TaskSectionProps {
  id: string;
  title: string;
  operations: readonly Operation[];
  toolActionsBlocked: boolean;
  busyToolIds: ReadonlySet<ToolId>;
  toolOf: (id: Operation["tool"]) => Tool | null;
  onOpenTool: (tool: Tool) => void;
  cancellingOperationId: string | null;
  onCancel: (operation: Operation) => void;
  onRecoverUpdate: (operation: Operation, tool: Tool, version: string) => void;
  onViewDetails: (details: TaskCenterDetails) => void;
}

function TaskSection({
  id,
  title,
  operations,
  toolActionsBlocked,
  busyToolIds,
  toolOf,
  onOpenTool,
  cancellingOperationId,
  onCancel,
  onRecoverUpdate,
  onViewDetails,
}: TaskSectionProps) {
  return (
    <section aria-labelledby={id}>
      <h3
        id={id}
        className="px-1 text-caption font-medium uppercase tracking-wide text-content-muted"
      >
        {title}
      </h3>
      <ul className="mt-2 space-y-2">
        {operations.map((operation) => (
          <TaskCenterOperationItem
            key={operation.id}
            operation={operation}
            tool={toolOf(operation.tool)}
            openBlocked={toolActionsBlocked}
            recoveryBlocked={
              toolActionsBlocked ||
              (operation.tool !== null && busyToolIds.has(operation.tool))
            }
            onOpenTool={onOpenTool}
            cancelPending={cancellingOperationId === operation.id}
            onCancel={onCancel}
            onRecoverUpdate={onRecoverUpdate}
            onViewDetails={onViewDetails}
          />
        ))}
      </ul>
    </section>
  );
}

export interface TaskCenterPanelBodyProps {
  operations: readonly Operation[];
  pending: boolean;
  unavailable: boolean;
  refreshFailed: boolean;
  refreshing: boolean;
  toolsUnavailable: boolean;
  toolsRefreshFailed: boolean;
  toolsRefreshing: boolean;
  toolActionsBlocked: boolean;
  busyToolIds: ReadonlySet<ToolId>;
  activeHeadingId: string;
  recentHeadingId: string;
  toolOf: (id: Operation["tool"]) => Tool | null;
  retryButtonRef: Ref<HTMLButtonElement>;
  toolRetryButtonRef: Ref<HTMLButtonElement>;
  onRetry: () => void;
  onRetryTools: () => void;
  onOpenTool: (tool: Tool) => void;
  cancellingOperationId: string | null;
  onCancel: (operation: Operation) => void;
  onRecoverUpdate: (operation: Operation, tool: Tool, version: string) => void;
  onViewDetails: (details: TaskCenterDetails) => void;
}

export function TaskCenterPanelBody({
  operations,
  pending,
  unavailable,
  refreshFailed,
  refreshing,
  toolsUnavailable,
  toolsRefreshFailed,
  toolsRefreshing,
  toolActionsBlocked,
  busyToolIds,
  activeHeadingId,
  recentHeadingId,
  toolOf,
  retryButtonRef,
  toolRetryButtonRef,
  onRetry,
  onRetryTools,
  onOpenTool,
  cancellingOperationId,
  onCancel,
  onRecoverUpdate,
  onViewDetails,
}: TaskCenterPanelBodyProps) {
  const { t } = useTranslation();
  const active = operations.filter(
    (operation) => !isTerminal(operation.status),
  );
  const recent = operations.filter((operation) => isTerminal(operation.status));

  return (
    <div
      className="scrollbar-subtle min-h-0 overflow-y-auto p-3"
      aria-busy={pending || undefined}
    >
      {pending ? (
        <DetectionStatus
          className="min-h-32"
          label={t("taskCenter.loading.label")}
        />
      ) : null}

      {unavailable ? (
        <div className="rounded-xl border border-danger/30 bg-danger/5 p-4">
          <div className="flex items-start gap-3">
            <TriangleAlert
              className="mt-0.5 h-5 w-5 shrink-0 text-danger"
              aria-hidden="true"
            />
            <div className="min-w-0 flex-1">
              <div role="alert">
                <p className="text-body text-content">
                  {t("taskCenter.unavailable.title")}
                </p>
                <p className="mt-1 text-caption text-content-muted">
                  {t("taskCenter.unavailable.description")}
                </p>
              </div>
              <Button
                ref={retryButtonRef}
                variant="secondary"
                size="sm"
                className="mt-3"
                loading={refreshing}
                onClick={onRetry}
              >
                {!refreshing ? (
                  <RefreshCw className="h-4 w-4" aria-hidden="true" />
                ) : null}
                {t("taskCenter.unavailable.retry")}
              </Button>
            </div>
          </div>
        </div>
      ) : null}

      {refreshFailed ? (
        <aside
          role="alert"
          aria-label={t("taskCenter.refreshError.title")}
          aria-busy={refreshing || undefined}
          className="mb-3 flex flex-col gap-3 rounded-xl border border-warning/30 bg-warning/10 p-4"
        >
          <div className="flex min-w-0 items-start gap-3">
            <TriangleAlert
              className="mt-0.5 h-5 w-5 shrink-0 text-warning"
              aria-hidden="true"
            />
            <div className="min-w-0">
              <p className="text-body font-medium text-content">
                {t("taskCenter.refreshError.title")}
              </p>
              <p className="mt-1 text-caption leading-5 text-content-muted">
                {t("taskCenter.refreshError.description")}
              </p>
            </div>
          </div>
          <Button
            ref={retryButtonRef}
            variant="secondary"
            size="sm"
            className="self-start"
            loading={refreshing}
            onClick={onRetry}
          >
            <RefreshCw className="h-4 w-4" aria-hidden="true" />
            {t("taskCenter.refreshError.retry")}
          </Button>
        </aside>
      ) : null}

      {!pending &&
      !unavailable &&
      operations.length > 0 &&
      (toolsUnavailable || toolsRefreshFailed) ? (
        <TaskToolInventoryNotice
          retained={toolsRefreshFailed}
          refreshing={toolsRefreshing}
          retryButtonRef={toolRetryButtonRef}
          onRetry={onRetryTools}
        />
      ) : null}

      {!pending && !unavailable && operations.length === 0 ? (
        <EmptyState
          icon={ClipboardList}
          title={t("taskCenter.empty.title")}
          description={t("taskCenter.empty.description")}
          className="py-10"
        />
      ) : null}

      {!pending && !unavailable && operations.length > 0 ? (
        <div className="space-y-5">
          {active.length > 0 ? (
            <TaskSection
              id={activeHeadingId}
              title={t("taskCenter.sections.active")}
              operations={active}
              toolActionsBlocked={toolActionsBlocked}
              busyToolIds={busyToolIds}
              toolOf={toolOf}
              onOpenTool={onOpenTool}
              cancellingOperationId={cancellingOperationId}
              onCancel={onCancel}
              onRecoverUpdate={onRecoverUpdate}
              onViewDetails={onViewDetails}
            />
          ) : null}
          {recent.length > 0 ? (
            <TaskSection
              id={recentHeadingId}
              title={t("taskCenter.sections.recent")}
              operations={recent}
              toolActionsBlocked={toolActionsBlocked}
              busyToolIds={busyToolIds}
              toolOf={toolOf}
              onOpenTool={onOpenTool}
              cancellingOperationId={cancellingOperationId}
              onCancel={onCancel}
              onRecoverUpdate={onRecoverUpdate}
              onViewDetails={onViewDetails}
            />
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
