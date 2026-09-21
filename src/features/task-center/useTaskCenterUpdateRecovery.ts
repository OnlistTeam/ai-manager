import { useState } from "react";
import type { Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import { useInstallToolVersion } from "@/features/tool-management";

export interface FailedUpdateRestoreRequest {
  operationId: string;
  toolId: Tool["id"];
  toolName: string;
  version: string;
}

export interface TaskCenterUpdateRecovery {
  /** The restore awaiting confirmation; null when closed. */
  request: FailedUpdateRestoreRequest | null;
  /** Still authorised by the fresh inventory, the task list and the tool's idleness. */
  valid: boolean;
  busy: boolean;
  error: Error | null;
  begin: (operation: Operation, tool: Tool, version: string) => void;
  setOpen: (open: boolean) => void;
  confirm: () => void;
}

/**
 * Restoring the version a failed update left broken. Authorisation is
 * re-checked on every render so a tool that changed underneath the dialog
 * locks its confirm button rather than acting on a stale snapshot.
 */
export function useTaskCenterUpdateRecovery(
  operations: readonly Operation[],
  toolOf: (id: Operation["tool"]) => Tool | null,
  busyToolIds: ReadonlySet<Tool["id"]>,
  toolActionsBlocked: boolean,
): TaskCenterUpdateRecovery {
  const restoreVersion = useInstallToolVersion({ notifyOnError: false });
  const [request, setRequest] = useState<FailedUpdateRestoreRequest | null>(
    null,
  );
  const recoveryTool = request ? toolOf(request.toolId) : null;
  const valid = Boolean(
    request &&
      recoveryTool?.status === "broken" &&
      recoveryTool.capabilities.canManageVersion &&
      !busyToolIds.has(request.toolId) &&
      operations.some(
        (operation) =>
          operation.id === request.operationId &&
          operation.kind === "update" &&
          operation.status === "failed" &&
          operation.tool === request.toolId &&
          operation.updateRecovery?.kind === "available" &&
          operation.updateRecovery.targetVersion === request.version,
      ),
  );

  return {
    request,
    valid,
    busy: restoreVersion.isPending,
    error: request ? restoreVersion.error : null,
    begin: (operation, tool, version) => {
      if (
        toolActionsBlocked ||
        busyToolIds.has(tool.id) ||
        tool.status !== "broken" ||
        !tool.capabilities.canManageVersion ||
        operation.kind !== "update" ||
        operation.status !== "failed" ||
        operation.tool !== tool.id ||
        operation.updateRecovery?.kind !== "available" ||
        operation.updateRecovery.targetVersion !== version
      ) {
        return;
      }
      restoreVersion.reset();
      setRequest({
        operationId: operation.id,
        toolId: tool.id,
        toolName: tool.name,
        version,
      });
    },
    setOpen: (open) => {
      if (open) return;
      restoreVersion.reset();
      setRequest(null);
    },
    confirm: () => {
      if (!request || !valid) return;
      restoreVersion.mutate(
        { tool: request.toolId, version: request.version },
        { onSuccess: () => setRequest(null) },
      );
    },
  };
}
