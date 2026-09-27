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
  /** Opens the Software page, where updates are reviewed. */
  onOpenUpdates: () => void;
}

const STATUS_ICON = {
  ready: { icon: CircleCheck, className: "text-success" },
  attention: { icon: TriangleAlert, className: "text-warning" },
  action: { icon: CircleAlert, className: "text-danger" },
} as const;

/** Shared frame of every status line state, so the page never jumps between them. */
export const STATUS_LINE_CLASS =
  "flex min-h-8 min-w-0 flex-wrap items-center gap-x-3 gap-y-1 px-1";

function formatCheckedAt(timestamp: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(timestamp);
}

/**
 * The environment verdict as one quiet line: a three-tier status and one
 * sentence, when it was checked, and the single next step. The findings below
 * name the individual problems; this line only counts them. Updates are only
 * counted here, as a way to the Software page; Home itself updates nothing.
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
  onOpenUpdates,
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
    <div data-status={summary.status} className={STATUS_LINE_CLASS}>
      <StatusIcon
        className={`h-4 w-4 shrink-0 ${visual.className}`}
        aria-hidden="true"
      />
      <span className="sr-only">{t(`ds.status.${summary.status}`)}</span>
      <h1 id="home-status-title" className="text-body font-medium text-content">
        {headline}
      </h1>
      {summary.updatableCount > 0 ? (
        <Button
          variant="ghost"
          size="xs"
          className="-mx-1.5 h-6 px-1.5"
          onClick={onOpenUpdates}
        >
          <RefreshCw className="h-3.5 w-3.5 text-warning" aria-hidden="true" />
          {t("home.status.updatesAvailable", {
            count: summary.updatableCount,
          })}
        </Button>
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

      <div className="ml-auto flex flex-wrap items-center justify-end gap-1">
        <Button
          variant="ghost"
          size="xs"
          loading={rechecking}
          onClick={onRecheck}
        >
          <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
          {t("home.health.recheck")}
        </Button>
        {summary.status === "ready" && startTool ? (
          <Button
            variant="secondary"
            size="xs"
            onClick={() => onStart(startTool)}
          >
            <FolderOpen className="h-3.5 w-3.5" aria-hidden="true" />
            {t("home.status.startTool", { name: startTool.name })}
          </Button>
        ) : summary.status !== "ready" ? (
          <Button
            variant="secondary"
            size="xs"
            onClick={() => onReview(recommendation?.destination ?? "tools")}
          >
            {reviewLabel}
            <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
          </Button>
        ) : null}
      </div>
    </div>
  );
}
