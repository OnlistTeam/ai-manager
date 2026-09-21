import { useTranslation } from "react-i18next";
import { OperationLogDetails, type Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import { payloadToErrorCopy } from "@/shared/lib/nativeError";
import { Button } from "@/shared/ui/Button";
import { ProgressTask } from "@/shared/ui/ProgressTask";
import { UpdateRecoveryNotice } from "./UpdateRecoveryNotice";

export interface ToolOperationProgressProps {
  operation: Operation;
  toolName: string;
  tool?: Tool | null;
  retryDisabled?: boolean;
  onDismiss?: () => void;
  onRetry?: () => void;
  onRestoreVersion?: (version: string) => void;
  onCancel?: () => void;
  cancelPending?: boolean;
}

/**
 * The card keeps the product status readable while the shared, collapsed log
 * lets technical users inspect the native-redacted process output in place.
 */
export function ToolOperationProgress({
  operation,
  toolName,
  tool = null,
  retryDisabled = false,
  onDismiss,
  onRetry,
  onRestoreVersion,
  onCancel,
  cancelPending = false,
}: ToolOperationProgressProps) {
  const { t } = useTranslation();
  const taskName = operation.extension
    ? t(`taskCenter.extension.${operation.extension.kind}.${operation.kind}`, {
        name: operation.extension.name,
      })
    : t(`taskCenter.kind.${operation.kind}`, { name: toolName });
  const error = operation.error ? payloadToErrorCopy(operation.error) : null;
  const detailKey = error?.messageKey ?? operation.messageKey;
  const failed = operation.status === "failed";

  return (
    <div
      data-tool-operation={operation.kind}
      className="rounded-lg border border-brand/20 bg-brand/5 px-3 py-3"
    >
      <ProgressTask
        name={taskName}
        progress={operation.progress}
        status={operation.status}
        detail={detailKey ? t(detailKey) : undefined}
        progressLabel={taskName}
        onCancel={operation.canCancel ? onCancel : undefined}
        cancelPending={cancelPending}
        className="py-0"
      />
      <OperationLogDetails logs={operation.logs} />
      <UpdateRecoveryNotice
        operation={operation}
        tool={tool}
        blocked={retryDisabled}
        onRestore={onRestoreVersion}
      />
      {failed && (onRetry || onDismiss) ? (
        <div className="mt-3 flex flex-wrap items-center gap-2 border-t border-hairline pt-3">
          {onRetry ? (
            <Button
              variant="secondary"
              size="sm"
              disabled={retryDisabled}
              onClick={onRetry}
            >
              {t("ds.action.retry")}
            </Button>
          ) : null}
          {onDismiss ? (
            <Button variant="ghost" size="sm" onClick={onDismiss}>
              {t("ds.action.close")}
            </Button>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
