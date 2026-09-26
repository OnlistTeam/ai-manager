import { AlertTriangle } from "lucide-react";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  isLiveRoutingOn,
  useRoutingOverview,
  useSetLiveRoutingMode,
} from "@/entities/routing";
import { ConfirmActionModal } from "@/features/tool-management";
import { cn } from "@/shared/ui/cn";
import { Switch } from "@/shared/ui/Switch";
import { liveRoutingCandidates } from "./liveRoutingFormat";

export interface LiveRoutingSwitchProps {
  /** Another routing change is in flight on the same page. */
  disabled?: boolean;
  className?: string;
}

/**
 * One switch for live routing (ADR-0050). On takes over every tool with a
 * current service after a confirmation; off stops and restores all of them.
 * Tools that could not be taken over are listed below the switch.
 */
export function LiveRoutingSwitch({
  disabled = false,
  className,
}: LiveRoutingSwitchProps) {
  const { t } = useTranslation();
  const overview = useRoutingOverview();
  const setLive = useSetLiveRoutingMode();
  const [confirming, setConfirming] = useState<"on" | "off" | null>(null);
  const descriptionId = useId();
  const data = overview.data;
  const on = data ? isLiveRoutingOn(data) : false;
  const candidates = data ? liveRoutingCandidates(data) : [];
  const cannotStart = !on && candidates.length === 0;
  const separator = t("routing.live.listSeparator");
  const failures = setLive.data?.failures ?? [];

  function ask(next: boolean): void {
    setLive.reset();
    setConfirming(next ? "on" : "off");
  }

  function confirm(): void {
    if (confirming === null) return;
    setLive.mutate(confirming === "on", {
      onSuccess: () => setConfirming(null),
    });
  }

  return (
    <div className={cn("flex min-w-0 flex-col gap-2", className)}>
      <div className="flex items-start gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-body font-medium text-content">
            {t("routing.live.title")}
          </p>
          <p
            id={descriptionId}
            className="mt-0.5 text-caption text-content-muted"
          >
            {cannotStart
              ? t("routing.live.noTargets", { page: t("nav.services") })
              : t("routing.live.description")}
          </p>
        </div>
        <Switch
          checked={on}
          disabled={!data || disabled || setLive.isPending || cannotStart}
          aria-label={t("routing.live.title")}
          aria-describedby={descriptionId}
          onCheckedChange={ask}
        />
      </div>

      {failures.length > 0 ? (
        <div
          role="alert"
          className="flex items-start gap-2 rounded-lg border border-warning/25 bg-warning/5 px-3 py-2 text-caption"
        >
          <AlertTriangle
            className="mt-0.5 h-3.5 w-3.5 shrink-0 text-warning"
            aria-hidden="true"
          />
          <div className="min-w-0">
            <p className="font-medium text-content">
              {t(
                on
                  ? "routing.live.partial.title"
                  : "routing.live.partial.noneTitle",
              )}
            </p>
            <ul className="mt-0.5 text-content-muted">
              {failures.map((failure) => (
                <li key={failure.tool} data-live-failure={failure.tool}>
                  {t("routing.live.partial.item", {
                    name: t(`routing.tool.${failure.tool}`),
                    reason: t(failure.error.messageKey),
                  })}
                </li>
              ))}
            </ul>
          </div>
        </div>
      ) : null}

      <ConfirmActionModal
        open={confirming !== null}
        onOpenChange={(open) => {
          if (!open && !setLive.isPending) {
            setLive.reset();
            setConfirming(null);
          }
        }}
        title={t(
          confirming === "off"
            ? "routing.stop.title"
            : "routing.live.confirm.title",
        )}
        description={
          confirming === "off"
            ? t("routing.stop.description")
            : t("routing.live.confirm.description", {
                tools: candidates
                  .map((tool) => t(`routing.tool.${tool}`))
                  .join(separator),
              })
        }
        confirmLabel={t(
          confirming === "off"
            ? "routing.stop.confirm"
            : "routing.live.confirm.action",
        )}
        confirmTone={confirming === "off" ? "danger" : "primary"}
        busy={setLive.isPending}
        error={setLive.error}
        onConfirm={confirm}
      />
    </div>
  );
}
