import { FolderOpen, ScanSearch } from "lucide-react";
import { useTranslation } from "react-i18next";
import { OperationLogDetails, type Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import type { ErrorCopy } from "@/shared/lib/nativeError";
import { UpdateRecoveryNotice } from "@/features/tool-management";
import { Button } from "@/shared/ui/Button";
import { ProgressTask } from "@/shared/ui/ProgressTask";
import { ExtensionArtwork } from "@/shared/ui/ExtensionArtwork";
import { ToolArtwork } from "@/shared/ui/ToolArtwork";
import {
  canOpenCompletedTool,
  formatOperationTime,
  presentOperation,
} from "./operationPresentation";

export interface TaskCenterDetails {
  error: ErrorCopy;
  taskName: string;
}

interface TaskCenterOperationItemProps {
  operation: Operation;
  tool: Tool | null;
  openBlocked: boolean;
  recoveryBlocked: boolean;
  cancelPending: boolean;
  onOpenTool: (tool: Tool) => void;
  onCancel: (operation: Operation) => void;
  onRecoverUpdate: (operation: Operation, tool: Tool, version: string) => void;
  onViewDetails: (details: TaskCenterDetails) => void;
}

function timestampOf(operation: Operation): number | null {
  return operation.finishedAt ?? operation.startedAt;
}

/** A compact task row; bounded technical output stays behind an explicit disclosure. */
export function TaskCenterOperationItem({
  operation,
  tool,
  openBlocked,
  recoveryBlocked,
  cancelPending,
  onOpenTool,
  onCancel,
  onRecoverUpdate,
  onViewDetails,
}: TaskCenterOperationItemProps) {
  const { t, i18n } = useTranslation();
  const view = presentOperation(operation);
  const error = view.error;
  const toolName = tool?.name ?? operation.tool ?? "";
  const taskName = t(view.titleKey, {
    name: view.targetName ?? toolName,
  });
  const canOpen = canOpenCompletedTool(operation, tool);
  const timestamp = view.active ? null : timestampOf(operation);
  const locale = i18n.resolvedLanguage || i18n.language || "en";
  const timeLabel =
    timestamp !== null
      ? formatOperationTime(timestamp, Date.now(), locale)
      : null;
  const date = timestamp !== null ? new Date(timestamp) : null;
  const validDate = date && !Number.isNaN(date.getTime()) ? date : null;

  return (
    <li className="rounded-xl border border-hairline bg-layer-1 p-3 shadow-sm">
      <div className="flex items-start gap-3">
        {operation.extension ? (
          <ExtensionArtwork
            kind={operation.extension.kind}
            active={view.active}
            className="h-10 w-10 rounded-lg"
          />
        ) : operation.tool ? (
          <ToolArtwork
            toolId={operation.tool}
            className="h-10 w-10 rounded-lg"
          />
        ) : (
          <span
            aria-hidden="true"
            className="flex h-10 w-10 shrink-0 items-center justify-center rounded-lg border border-hairline bg-layer-1 text-brand shadow-sm"
          >
            <ScanSearch className="h-5 w-5" />
          </span>
        )}

        <div className="min-w-0 flex-1">
          <ProgressTask
            name={taskName}
            progress={operation.progress}
            status={operation.status}
            detail={view.detailKey ? t(view.detailKey) : undefined}
            progressLabel={taskName}
            onCancel={
              operation.canCancel ? () => onCancel(operation) : undefined
            }
            cancelPending={cancelPending}
            className="py-0"
          />

          {timeLabel || error || canOpen ? (
            <div className="mt-2 flex min-h-7 items-center justify-between gap-2">
              {timeLabel && validDate ? (
                <time
                  dateTime={validDate.toISOString()}
                  title={new Intl.DateTimeFormat(locale, {
                    dateStyle: "medium",
                    timeStyle: "short",
                  }).format(validDate)}
                  className="text-caption text-content-muted"
                >
                  {timeLabel}
                </time>
              ) : (
                <span />
              )}
              {error ? (
                <Button
                  variant="ghost"
                  size="sm"
                  className="-mr-2 h-7 px-2"
                  aria-label={t("taskCenter.viewDetailsFor", {
                    name: taskName,
                  })}
                  onClick={() => onViewDetails({ error, taskName })}
                >
                  {t("taskCenter.viewDetails")}
                </Button>
              ) : null}
              {canOpen ? (
                <Button
                  variant="secondary"
                  size="sm"
                  className="-mr-1 h-7 px-2.5"
                  aria-label={t("tools.open.title", { name: tool.name })}
                  disabled={openBlocked}
                  onClick={() => {
                    if (!openBlocked) onOpenTool(tool);
                  }}
                >
                  <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />
                  {t("ds.action.open")}
                </Button>
              ) : null}
            </div>
          ) : null}
          <OperationLogDetails logs={operation.logs} />
          <UpdateRecoveryNotice
            operation={operation}
            tool={tool}
            blocked={recoveryBlocked}
            onRestore={
              tool
                ? (version) => onRecoverUpdate(operation, tool, version)
                : undefined
            }
          />
        </div>
      </div>
    </li>
  );
}
