import { TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { PortabilityReason } from "@/entities/extension";
import { FOCUS_RING } from "@/shared/ui/focusRing";
import { cn } from "@/shared/ui/cn";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/shared/ui/Tooltip";
import {
  mayNotWorkIn,
  portabilityAffected,
  type PortabilityTarget,
} from "./extensionPortability";
import type { UnifiedExtensionRow } from "./unifiedExtensionRows";

const REASON_KEYS: Record<PortabilityReason, string> = {
  relativePath: "extensions.portability.relativePath",
  envReference: "extensions.portability.envReference",
  toolHome: "extensions.portability.toolHome",
};

export interface PortabilityHintProps {
  row: UnifiedExtensionRow;
  targets: readonly PortabilityTarget[];
}

/**
 * One small warning mark beside the name when the item is on somewhere it
 * may not work as it was set up (ADR-0062). The sentence says what may break
 * and what to do; there is no badge text, so the row stays calm.
 */
export function PortabilityHint({ row, targets }: PortabilityHintProps) {
  const { t } = useTranslation();
  const affected = portabilityAffected(row, targets);
  if (row.portability === null || affected.length === 0) return null;
  const sentence = t(REASON_KEYS[row.portability.reason], {
    tools: affected.map((target) => target.name),
  });

  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <button
          type="button"
          aria-label={sentence}
          className={cn(
            "flex h-5 w-5 shrink-0 items-center justify-center rounded text-warning",
            FOCUS_RING,
          )}
        >
          <TriangleAlert className="h-3.5 w-3.5" aria-hidden="true" />
        </button>
      </TooltipTrigger>
      <TooltipContent side="bottom">{sentence}</TooltipContent>
    </Tooltip>
  );
}

/** The caution an off switch shows before it is turned on, if any. */
export function usePortabilityCaution(
  row: UnifiedExtensionRow,
): (target: PortabilityTarget) => string | null {
  const { t } = useTranslation();
  return (target) =>
    mayNotWorkIn(row, target)
      ? t("extensions.portability.switchCaution")
      : null;
}
