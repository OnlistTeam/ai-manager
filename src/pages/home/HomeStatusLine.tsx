import {
  ArrowRight,
  CircleAlert,
  CircleCheck,
  FolderOpen,
  LoaderCircle,
  RefreshCw,
  TriangleAlert,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Tool } from "@/entities/tool";
import type { QuickCheckSummary } from "@/features/health";
import { Button } from "@/shared/ui/Button";
import { Card } from "@/shared/ui/Card";
import type { HomeDestination, HomeRecommendation } from "./homeRecommendation";

export interface HomeStatusLineProps {
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

const STATUS_ICON = {
  ready: { icon: CircleCheck, className: "text-success" },
  attention: { icon: TriangleAlert, className: "text-warning" },
  action: { icon: CircleAlert, className: "text-danger" },
} as const;

/** Shared frame of every status line state, so the page never jumps between them. */
export const STATUS_LINE_CLASS =
  "flex min-h-14 min-w-0 flex-wrap items-center gap-x-3 gap-y-2 px-4 py-2.5";

function formatCheckedAt(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(timestamp);
}

/**
 * The environment verdict in one line: a three-tier status and one sentence,
 * when it was checked, and the single next step. The findings below name the
 * individual problems; this line only counts them.
 */
export function HomeStatusLine({
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
}: HomeStatusLineProps) {
  const { t, i18n } = useTranslation();
  const hasTools = summary.installedCount > 0;
  const headline = !hasTools
    ? t("home.status.nothingInstalled")
    : summary.status === "ready"
      ? t("home.status.allGood")
      : t("home.status.needAttention", { count: summary.attentionCount });
  const visual = STATUS_ICON[summary.status];
  const StatusIcon = visual.icon;
  const checkedLine = checkingConnections
    ? t("home.health.checkingConnections")
    : checkedAt !== null
      ? t("home.health.lastChecked", {
          time: formatCheckedAt(
            checkedAt,
            i18n.resolvedLanguage ?? i18n.language,
          ),
        })
      : null;
  const reviewLabel = recommendation
    ? t(recommendation.actionKey)
    : hasTools
      ? t("home.status.review")
      : t("home.tools.install");

  return (
    <Card
      padding="none"
      data-status={summary.status}
      className={STATUS_LINE_CLASS}
    >
      <StatusIcon
        className={`h-5 w-5 shrink-0 ${visual.className}`}
        aria-hidden="true"
      />
      <span className="sr-only">{t(`ds.status.${summary.status}`)}</span>
      <h1 id="home-status-title" className="text-heading text-content">
        {headline}
      </h1>
      {summary.status !== "ready" && summary.updatableCount > 0 ? (
        <span className="inline-flex items-center gap-1 text-caption text-content-muted">
          <RefreshCw className="h-3.5 w-3.5 text-warning" aria-hidden="true" />
          {t("home.status.updatesAvailable", {
            count: summary.updatableCount,
          })}
        </span>
      ) : null}
      {checkedLine ? (
        <span className="inline-flex items-center gap-1 text-caption text-content-muted">
          {checkingConnections ? (
            <LoaderCircle
              className="h-3.5 w-3.5 motion-safe:animate-spin"
              aria-hidden="true"
            />
          ) : null}
          {checkedLine}
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

      <div className="ml-auto flex flex-wrap items-center justify-end gap-2">
        <Button
          variant="ghost"
          size="sm"
          loading={rechecking}
          onClick={onRecheck}
        >
          <RefreshCw className="h-4 w-4" aria-hidden="true" />
          {t("home.health.recheck")}
        </Button>
        {summary.status === "ready" && startTool ? (
          <Button size="sm" onClick={() => onStart(startTool)}>
            <FolderOpen className="h-4 w-4" aria-hidden="true" />
            {t("home.status.startTool", { name: startTool.name })}
          </Button>
        ) : summary.status !== "ready" ? (
          <Button
            size="sm"
            onClick={() => onReview(recommendation?.destination ?? "tools")}
          >
            {reviewLabel}
            <ArrowRight className="h-4 w-4" aria-hidden="true" />
          </Button>
        ) : null}
      </div>
    </Card>
  );
}
