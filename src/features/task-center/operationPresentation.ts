import { isTerminal, type Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import { payloadToErrorCopy, type ErrorCopy } from "@/shared/lib/nativeError";

export interface OperationPresentation {
  titleKey: string;
  targetName: string | null;
  /** i18n key for the secondary line. On failure it comes from error, not a leftover phase. */
  detailKey: string | null;
  error: ErrorCopy | null;
  active: boolean;
}

/**
 * Spec §42: **on failure, error always wins.** The backend doesn't clear
 * `Operation.message_key` when a task fails; that phase key is just the
 * last phase the task was in before it stopped (usually "Installing").
 * Showing it as the failure reason would leave the user reading a
 * self-contradictory pair of lines like "Failed / Installing". Fall back
 * to phase only when error is missing — still better than saying nothing.
 */
export function presentOperation(operation: Operation): OperationPresentation {
  const error = operation.error ? payloadToErrorCopy(operation.error) : null;
  const extension = operation.extension;
  return {
    titleKey: extension
      ? `taskCenter.extension.${extension.kind}.${operation.kind}`
      : `taskCenter.kind.${operation.kind}`,
    targetName: extension?.name ?? null,
    detailKey: error
      ? error.messageKey
      : operation.status === "cancelled"
        ? null
        : operation.messageKey,
    error,
    active: !isTerminal(operation.status),
  };
}

/** §40's top-right count: how many tasks are still running. */
export function countActive(operations: readonly Operation[]): number {
  return operations.filter((operation) => !isTerminal(operation.status)).length;
}

/**
 * §31 completion action: operation success is necessary but not sufficient.
 * The refreshed tool inventory must independently confirm a launchable install.
 */
export function canOpenCompletedTool(
  operation: Operation,
  tool: Tool | null | undefined,
): tool is Tool {
  return Boolean(
    tool &&
      operation.status === "success" &&
      (operation.kind === "install" ||
        operation.kind === "update" ||
        operation.kind === "changeVersion") &&
      operation.extension === null &&
      operation.tool === tool.id &&
      tool.capabilities.canLaunch &&
      (tool.status === "installed" || tool.status === "updateAvailable"),
  );
}

/**
 * Friendly relative time for recent work. Older records use a compact date so
 * the wording never turns into an unhelpful "hundreds of hours ago" label.
 */
export function formatOperationTime(
  timestamp: number,
  now: number,
  locale: string,
): string | null {
  const date = new Date(timestamp);
  if (!Number.isFinite(timestamp) || Number.isNaN(date.getTime())) return null;

  const delta = timestamp - now;
  const distance = Math.abs(delta);
  const relative = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  if (distance < 60_000) return relative.format(0, "second");
  if (distance < 3_600_000) {
    return relative.format(Math.round(delta / 60_000), "minute");
  }
  if (distance < 86_400_000) {
    return relative.format(Math.round(delta / 3_600_000), "hour");
  }
  if (distance < 604_800_000) {
    return relative.format(Math.round(delta / 86_400_000), "day");
  }

  const includeYear = date.getFullYear() !== new Date(now).getFullYear();
  return new Intl.DateTimeFormat(locale, {
    month: "short",
    day: "numeric",
    ...(includeYear ? { year: "numeric" as const } : {}),
  }).format(date);
}
