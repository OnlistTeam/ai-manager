import { useState } from "react";
import type { Operation } from "@/entities/operation";
import type { Tool, ToolId } from "@/entities/tool";
import { useInstallToolVersion } from "@/features/tool-management";

export interface UpdateRecoveryRequest {
  tool: Tool;
  operationId: string;
  version: string;
}

export interface ToolVersionFlows {
  /** The tool whose version picker is open; null when closed. */
  versioning: Tool | null;
  versioningError: Error | null;
  /** A failed update's restore request awaiting confirmation; null when closed. */
  recovering: UpdateRecoveryRequest | null;
  recoveryError: Error | null;
  /** The restore is still authorised by both the fresh inventory and the task list. */
  recoveryValid: boolean;
  submitting: boolean;
  /** Which tool the in-flight version install belongs to, for the card's busy state. */
  submittingTool: ToolId | null;
  beginVersioning: (tool: Tool) => void;
  beginRecovery: (tool: Tool, operation: Operation, version: string) => void;
  setVersioningOpen: (open: boolean) => void;
  setRecoveryOpen: (open: boolean) => void;
  confirmVersion: (version: string) => void;
  confirmRecovery: () => void;
}

/**
 * The two dialogs that install a specific version share one mutation, so
 * opening either closes the other and resets the shared error.
 */
export function useToolVersionFlows(
  tools: readonly Tool[] | undefined,
  operations: readonly Operation[],
): ToolVersionFlows {
  const installVersion = useInstallToolVersion({ notifyOnError: false });
  const [versioning, setVersioning] = useState<Tool | null>(null);
  const [recovering, setRecovering] = useState<UpdateRecoveryRequest | null>(
    null,
  );
  const recoveryValid = Boolean(
    recovering &&
      tools?.some(
        (tool) =>
          tool.id === recovering.tool.id &&
          tool.status === "broken" &&
          tool.capabilities.canManageVersion,
      ) &&
      operations.some(
        (operation) =>
          operation.id === recovering.operationId &&
          operation.status === "failed" &&
          operation.kind === "update" &&
          operation.tool === recovering.tool.id &&
          operation.updateRecovery?.kind === "available" &&
          operation.updateRecovery.targetVersion === recovering.version,
      ),
  );

  return {
    versioning,
    versioningError: versioning ? installVersion.error : null,
    recovering,
    recoveryError: recovering ? installVersion.error : null,
    recoveryValid,
    submitting: installVersion.isPending,
    submittingTool:
      installVersion.isPending && installVersion.variables
        ? installVersion.variables.tool
        : null,
    beginVersioning: (tool) => {
      installVersion.reset();
      setRecovering(null);
      setVersioning(tool);
    },
    beginRecovery: (tool, operation, version) => {
      if (
        operation.kind !== "update" ||
        operation.status !== "failed" ||
        operation.tool !== tool.id ||
        operation.updateRecovery?.kind !== "available" ||
        operation.updateRecovery.targetVersion !== version ||
        tool.status !== "broken" ||
        !tool.capabilities.canManageVersion
      ) {
        return;
      }
      installVersion.reset();
      setVersioning(null);
      setRecovering({ tool, operationId: operation.id, version });
    },
    setVersioningOpen: (open) => {
      if (open) return;
      installVersion.reset();
      setVersioning(null);
    },
    setRecoveryOpen: (open) => {
      if (open) return;
      installVersion.reset();
      setRecovering(null);
    },
    confirmVersion: (version) => {
      if (!versioning) return;
      installVersion.mutate(
        { tool: versioning.id, version },
        { onSuccess: () => setVersioning(null) },
      );
    },
    confirmRecovery: () => {
      if (!recovering || !recoveryValid) return;
      installVersion.mutate(
        { tool: recovering.tool.id, version: recovering.version },
        { onSuccess: () => setRecovering(null) },
      );
    },
  };
}
