import type { TFunction } from "i18next";
import type { RoutingTraceEntry } from "@/entities/routing";
import type { FlightPlan } from "./flightPlan";
import { formatDuration, serviceLabel } from "./liveRoutingFormat";

export interface CaptionOptions {
  t: TFunction;
  language: string;
  hideEmails: boolean;
}

/**
 * One plain sentence for the step of `entry` the stage is showing: the leg
 * in flight, or the last one flown. `done` counts the legs already flown.
 */
export function liveCaption(
  entry: RoutingTraceEntry,
  plan: FlightPlan,
  done: number,
  { t, language, hideEmails }: CaptionOptions,
): string {
  const leg = plan.legs[Math.min(done, plan.legs.length - 1)];
  const tool = t(`routing.tool.${entry.tool}`);
  const nameAt = (index: number) =>
    serviceLabel(entry.attempts[index]?.providerName ?? "", hideEmails);
  const reasonAt = (index: number) =>
    t(`routing.live.errorCategory.${entry.attempts[index]?.error ?? "other"}`);

  if (!leg || leg.kind === "send") {
    const next = plan.legs[1];
    return next?.kind === "try" && next.attempt === 0
      ? t("routing.live.caption.sent", { tool, name: nameAt(0) })
      : t("routing.live.caption.choosing", { tool });
  }

  const index = leg.attempt ?? entry.attempts.length - 1;
  switch (leg.kind) {
    case "try": {
      const previous = entry.attempts[index - 1];
      if (previous?.outcome === "failed") {
        return t("routing.live.caption.rerouted", {
          previous: nameAt(index - 1),
          reason: reasonAt(index - 1),
          name: nameAt(index),
        });
      }
      if (previous?.outcome === "skipped") {
        return t("routing.live.caption.skipped", {
          previous: nameAt(index - 1),
          name: nameAt(index),
        });
      }
      return t("routing.live.caption.sent", { tool, name: nameAt(index) });
    }
    case "skip":
      return t("routing.live.caption.passedOver", { name: nameAt(index) });
    case "bounce":
      return t("routing.live.caption.failedTry", {
        name: nameAt(index),
        reason: reasonAt(index),
      });
    case "answer":
    case "deliver":
      return t("routing.live.caption.answered", {
        name: nameAt(index),
        duration: formatDuration(
          entry.totalMs ?? entry.attempts[index]?.ms ?? 0,
          language,
        ),
      });
    case "giveUp":
      return t("routing.live.caption.gaveUp", {
        tool,
        reason: t(`routing.live.errorCategory.${entry.error ?? "other"}`),
      });
  }
}
