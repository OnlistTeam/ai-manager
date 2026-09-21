import type { Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";

export type UpdateRecoveryAdvisoryReason = Exclude<
  NonNullable<Operation["updateRecovery"]>["kind"],
  "available"
>;

export type UpdateRecoveryPresentation =
  | { kind: "available"; version: string; actionable: boolean }
  | { kind: "advisory"; reason: UpdateRecoveryAdvisoryReason };

/**
 * The operation is evidence from the failed native update; the refreshed Tool
 * is a second authority check. Once the tool is no longer Broken, the recovery
 * prompt is stale and disappears instead of becoming a generic downgrade.
 */
export function updateRecoveryPresentation(
  operation: Operation,
  tool: Tool | null,
): UpdateRecoveryPresentation | null {
  if (
    operation.kind !== "update" ||
    operation.status !== "failed" ||
    operation.extension !== null ||
    operation.tool === null ||
    operation.updateRecovery === null
  ) {
    return null;
  }
  if (
    tool !== null &&
    (tool.id !== operation.tool || tool.status !== "broken")
  ) {
    return null;
  }

  const recovery = operation.updateRecovery;
  if (recovery.kind !== "available") {
    return { kind: "advisory", reason: recovery.kind };
  }
  if (tool !== null && !tool.capabilities.canManageVersion) {
    return { kind: "advisory", reason: "ownerUnsupported" };
  }
  return {
    kind: "available",
    version: recovery.targetVersion,
    actionable: tool !== null,
  };
}
