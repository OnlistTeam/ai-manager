import { ArrowRight, Check, SkipForward, X } from "lucide-react";
import { Fragment } from "react";
import { useTranslation } from "react-i18next";
import type { RoutingTraceAttempt } from "@/entities/routing";
import { cn } from "@/shared/ui/cn";
import { maskEmails } from "./liveRoutingFormat";

const OUTCOME_ICON = { ok: Check, failed: X, skipped: SkipForward } as const;
const OUTCOME_TONE = {
  ok: "text-success",
  failed: "text-danger",
  skipped: "text-content-muted",
} as const;

/** `Service A ✕ → Service B ✓`: every service tried, in order. */
export function LiveRoutingAttempts({
  attempts,
}: {
  attempts: readonly RoutingTraceAttempt[];
}) {
  const { t } = useTranslation();

  if (attempts.length === 0) {
    return (
      <span className="truncate text-content-muted">
        {t("routing.live.noAttempt")}
      </span>
    );
  }

  return (
    <span className="flex min-w-0 items-center gap-1 overflow-hidden">
      {attempts.map((attempt, index) => {
        const Icon = OUTCOME_ICON[attempt.outcome];
        const name = maskEmails(attempt.providerName);
        const reason = attempt.error
          ? t(`routing.live.errorCategory.${attempt.error}`)
          : "";
        const label = t(`routing.live.attempt.${attempt.outcome}`, {
          name,
          reason: attempt.httpStatus
            ? `${reason} · HTTP ${attempt.httpStatus}`
            : reason,
        });
        return (
          <Fragment key={`${attempt.providerId}-${index}`}>
            {index > 0 ? (
              <ArrowRight
                className="h-3 w-3 shrink-0 text-content-muted"
                aria-hidden="true"
              />
            ) : null}
            <span
              title={label}
              data-attempt-outcome={attempt.outcome}
              className={cn(
                "inline-flex min-w-0 items-center gap-0.5",
                attempt.outcome === "skipped" && "text-content-muted",
              )}
            >
              <span className="truncate" aria-hidden="true">
                {name}
              </span>
              <Icon
                className={cn(
                  "h-3 w-3 shrink-0",
                  OUTCOME_TONE[attempt.outcome],
                )}
                aria-hidden="true"
              />
              <span className="sr-only">{label}</span>
            </span>
          </Fragment>
        );
      })}
    </span>
  );
}
