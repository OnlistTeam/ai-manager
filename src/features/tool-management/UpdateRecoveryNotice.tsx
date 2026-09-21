import { RotateCcw, TriangleAlert } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Operation } from "@/entities/operation";
import type { Tool } from "@/entities/tool";
import { Button } from "@/shared/ui/Button";
import { updateRecoveryPresentation } from "./updateRecovery";

export interface UpdateRecoveryNoticeProps {
  operation: Operation;
  tool: Tool | null;
  blocked?: boolean;
  onRestore?: (version: string) => void;
}

/** Region-neutral, explicit recovery handoff for a natively verified Broken update. */
export function UpdateRecoveryNotice({
  operation,
  tool,
  blocked = false,
  onRestore,
}: UpdateRecoveryNoticeProps) {
  const { t } = useTranslation();
  const recovery = updateRecoveryPresentation(operation, tool);
  if (recovery === null) return null;

  const available = recovery.kind === "available";
  const description = available
    ? t("tools.updateRecovery.available", { version: recovery.version })
    : t(`tools.updateRecovery.advisory.${recovery.reason}`);

  return (
    <aside className="mt-3 rounded-lg border border-warning/30 bg-warning/10 p-3">
      <div className="flex items-start gap-2.5">
        <TriangleAlert
          className="mt-0.5 h-4 w-4 shrink-0 text-warning"
          aria-hidden="true"
        />
        <div className="min-w-0 flex-1">
          <p className="text-caption font-medium text-content">
            {t("tools.updateRecovery.title")}
          </p>
          <p className="mt-1 text-caption leading-5 text-content-muted">
            {description}
          </p>
          {available ? (
            <Button
              variant="secondary"
              size="sm"
              className="mt-2"
              disabled={blocked || !recovery.actionable || !onRestore}
              aria-label={t("tools.updateRecovery.namedAction", {
                name: tool?.name ?? operation.tool,
                version: recovery.version,
              })}
              onClick={() => onRestore?.(recovery.version)}
            >
              <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
              {t("tools.updateRecovery.action", { version: recovery.version })}
            </Button>
          ) : null}
        </div>
      </div>
    </aside>
  );
}
