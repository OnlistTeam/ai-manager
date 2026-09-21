import { Globe2, Radio, Shuffle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ProviderTestResult } from "@/entities/provider";
import { Badge } from "@/shared/ui/Badge";
import { Button } from "@/shared/ui/Button";
import { describeReachability } from "./testPresentation";

interface ProviderCheckOutcomeProps {
  result: ProviderTestResult;
  providerName: string;
  actionsBlocked: boolean;
  tryingNext?: boolean;
  tryNextUnavailable?: boolean;
  onTryNext?: () => void;
  onBrowseCompatible?: () => void;
}

export function ProviderCheckOutcome({
  result,
  providerName,
  actionsBlocked,
  tryingNext = false,
  tryNextUnavailable = false,
  onTryNext,
  onBrowseCompatible,
}: ProviderCheckOutcomeProps) {
  const { t } = useTranslation();
  const outcome = describeReachability(result);

  return (
    <div className="flex flex-col items-start gap-2.5">
      <Badge tone={outcome.tone} icon={Radio}>
        <span>{t(outcome.labelKey)}</span>
        {outcome.responseTimeMs === null ? null : (
          <span>
            {" "}
            · {t("services.test.latency", { ms: outcome.responseTimeMs })}
          </span>
        )}
      </Badge>

      {result.reachability === "failed" ? (
        <div className="flex w-full flex-col items-start gap-2 rounded-xl border border-warning/20 bg-warning/[0.06] p-3">
          <p className="text-caption leading-5 text-content-muted">
            {t("services.test.unreachableHint")}
          </p>
          <div className="flex flex-wrap gap-2">
            {onTryNext ? (
              <Button
                variant="secondary"
                size="sm"
                loading={tryingNext}
                disabled={actionsBlocked}
                aria-label={t("services.failover.tryNextNamed", {
                  name: providerName,
                })}
                onClick={onTryNext}
              >
                {tryingNext ? null : (
                  <Shuffle className="h-4 w-4" aria-hidden="true" />
                )}
                {t("services.failover.tryNext")}
              </Button>
            ) : null}
            <Button
              variant="ghost"
              size="sm"
              disabled={actionsBlocked || !onBrowseCompatible}
              aria-label={t("services.test.browseCompatibleNamed", {
                name: providerName,
              })}
              onClick={onBrowseCompatible}
            >
              <Globe2 className="h-4 w-4" aria-hidden="true" />
              {t("services.test.browseCompatible")}
            </Button>
          </div>
          {tryNextUnavailable ? (
            <p
              role="status"
              className="text-caption leading-5 text-content-muted"
            >
              {t("services.failover.noneAvailable")}
            </p>
          ) : null}
        </div>
      ) : null}
    </div>
  );
}
