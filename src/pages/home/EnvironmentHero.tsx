import {
  ArrowRight,
  CircleAlert,
  CircleCheck,
  FolderOpen,
  RefreshCw,
  TriangleAlert,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { TERMINAL_TURNTABLE } from "./terminalModel";
import type { Tool } from "@/entities/tool";
import type { QuickCheckSummary } from "@/features/health";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import { SpatialScene, type SpatialSceneTone } from "@/shared/ui/SpatialScene";
import { useSpatialPointer } from "@/shared/ui/useSpatialPointer";
import type { HomeDestination, HomeRecommendation } from "./homeRecommendation";

export interface EnvironmentHeroProps {
  summary: QuickCheckSummary;
  recommendation: HomeRecommendation | null;
  startTool: Tool | null;
  /** `null` before the first check finishes. */
  checkedAt: number | null;
  rechecking: boolean;
  checkingConnections: boolean;
  connectionCheckError: boolean;
  onRecheck: () => void;
  onReview: (destination: HomeDestination) => void;
  onStart: (tool: Tool) => void;
}

function formatCheckedAt(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(timestamp);
}

const STATUS_VISUAL = {
  ready: {
    icon: CircleCheck,
    tone: "success",
    iconClassName: "text-success",
  },
  attention: {
    icon: TriangleAlert,
    tone: "warning",
    iconClassName: "text-warning",
  },
  action: {
    icon: CircleAlert,
    tone: "danger",
    iconClassName: "text-danger",
  },
} as const satisfies Record<
  QuickCheckSummary["status"],
  {
    icon: typeof CircleCheck;
    tone: SpatialSceneTone;
    iconClassName: string;
  }
>;

/**
 * The single visual focal point of the home page. It only conveys a
 * three-tier status and real counts, not disguising the ring as a health
 * score.
 */
export function EnvironmentHero({
  summary,
  recommendation,
  startTool,
  checkedAt,
  rechecking,
  checkingConnections,
  connectionCheckError,
  onRecheck,
  onReview,
  onStart,
}: EnvironmentHeroProps) {
  const { t, i18n } = useTranslation();
  const hasTools = summary.installedCount > 0;
  const headline = !hasTools
    ? t("home.card.nothingInstalled")
    : summary.status === "ready"
      ? t("home.card.allGood")
      : t("home.card.needAttention", { count: summary.attentionCount });
  const destination = recommendation?.destination ?? "tools";
  const actionLabel = recommendation
    ? t(recommendation.actionKey)
    : hasTools
      ? t("home.card.review")
      : t("home.quickActions.installTool");
  const visual = STATUS_VISUAL[summary.status];
  const StatusIcon = visual.icon;
  const pointer = useSpatialPointer<HTMLDivElement>(TERMINAL_TURNTABLE);
  const statusLine = checkingConnections
    ? t("home.health.checkingConnections")
    : checkedAt !== null
      ? t("home.health.lastChecked", {
          time: formatCheckedAt(
            checkedAt,
            i18n.resolvedLanguage ?? i18n.language,
          ),
        })
      : null;

  return (
    <Card
      ref={pointer.ref}
      padding="none"
      data-model="environment"
      data-tone={visual.tone}
      data-spatial-stage=""
      className="environment-hero spatial-page-hero relative isolate min-h-[430px] min-w-0 overflow-hidden lg:min-h-[350px]"
      aria-labelledby="home-environment-title"
    >
      <span aria-hidden="true" className="spatial-page-hero__wash" />
      <span aria-hidden="true" className="spatial-page-hero__lines" />

      <div className="environment-hero__layout spatial-page-hero__layout min-h-[430px] lg:min-h-[350px]">
        <SpatialScene
          icon={StatusIcon}
          model="environment"
          turntable={TERMINAL_TURNTABLE}
          tone={visual.tone}
          size="hero"
          className="spatial-page-hero__scene"
        />

        <div className="spatial-page-hero__content">
          <div className="spatial-page-hero__eyebrow">
            <StatusIcon
              className={`h-4 w-4 shrink-0 ${visual.iconClassName}`}
              aria-hidden="true"
            />
            <span>{t("home.card.title")}</span>
            <span aria-hidden="true">·</span>
            <span>{t(`ds.status.${summary.status}`)}</span>
          </div>
          <h1 id="home-environment-title" className="spatial-page-hero__title">
            {headline}
          </h1>

          {hasTools ? (
            <p className="sr-only">{t("home.card.description")}</p>
          ) : (
            <p className="spatial-page-hero__description">
              {t("home.card.description")}
            </p>
          )}

          {summary.status === "ready" && startTool ? (
            <div className="environment-hero__start mt-6 flex flex-wrap items-center gap-3">
              <Button
                size="lg"
                className="environment-hero__primary rounded-lg"
                onClick={() => onStart(startTool)}
              >
                <FolderOpen className="h-4 w-4" aria-hidden="true" />
                {t("home.card.startTool", { name: startTool.name })}
              </Button>
            </div>
          ) : summary.status !== "ready" ? (
            <div
              data-slot="environment-hero-actions"
              className="environment-hero__actions mt-6 flex flex-wrap items-center gap-3"
            >
              {summary.updatableCount > 0 ? (
                <div className="environment-hero__update inline-flex h-12 items-center gap-2 rounded-lg border border-warning/20 bg-warning/10 px-5 text-body font-medium text-content shadow-sm">
                  <RefreshCw
                    className="h-4 w-4 text-warning"
                    aria-hidden="true"
                  />
                  {t("home.card.updatesAvailable", {
                    count: summary.updatableCount,
                  })}
                </div>
              ) : null}
              <Button
                size="lg"
                className="environment-hero__review rounded-lg"
                onClick={() => onReview(destination)}
              >
                {actionLabel}
                <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Button>
            </div>
          ) : null}

          <div className="mt-5 flex flex-wrap items-center gap-x-3 gap-y-2">
            <Button
              variant="ghost"
              size="sm"
              loading={rechecking}
              onClick={onRecheck}
            >
              <RefreshCw className="h-4 w-4" aria-hidden="true" />
              {t("home.health.recheck")}
            </Button>
            {statusLine ? (
              <span className="text-caption text-content-muted">
                {statusLine}
              </span>
            ) : null}
            {connectionCheckError ? (
              <span
                role="alert"
                aria-label={t("home.health.connectionError")}
                className="text-caption text-danger"
              >
                {t("home.health.connectionError")}
              </span>
            ) : null}
          </div>
        </div>
      </div>
    </Card>
  );
}
