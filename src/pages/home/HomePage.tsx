import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import { QuickCheckResolutionGuide } from "@/features/health";
import {
  firstLaunchableTool,
  useToolLaunchFlow,
} from "@/features/tool-management";
import { Card } from "@/shared/ui/Card";
import { HomeDialogs } from "./HomeDialogs";
import {
  HomeRefreshNotice,
  HomeStatusChecking,
  HomeUnavailableState,
} from "./HomeReadinessNotice";
import { HomeStatusLine } from "./HomeStatusLine";
import { HomeToolList } from "./HomeToolList";
import {
  recommendHomeAction,
  type HomeDestination,
} from "./homeRecommendation";
import { connectableTools, shownElsewhereOnHome } from "./homeToolConnection";
import { useHomeReadinessRecovery } from "./useHomeReadinessRecovery";

export interface HomePageProps {
  onOpenTools: () => void;
  onOpenServices: (toolId?: ToolId) => void;
  onOpenMcp: () => void;
}

/**
 * Which tool uses what (ADR-0053): one quiet status line, the findings that
 * need a next step, and one row per installed tool where its endpoint is
 * chosen. Adding and checking endpoints, updates and routing live on their
 * own pages.
 */
export function HomePage({
  onOpenTools,
  onOpenServices,
  onOpenMcp,
}: HomePageProps) {
  const { t } = useTranslation();
  const {
    check,
    pageRef,
    retryButtonRef,
    checkUnavailable,
    refreshFailed,
    retrying,
    actionsBlocked,
    retryHome,
  } = useHomeReadinessRecovery();
  const launch = useToolLaunchFlow();

  const list = check.tools.data ?? [];
  const summary = check.data;
  const rows = connectableTools(list);
  const rowToolIds = new Set<string>(rows.map((tool) => tool.id));
  const findings = summary
    ? summary.items.filter(
        (item) =>
          item.status !== "ready" &&
          item.resolution !== undefined &&
          !shownElsewhereOnHome(item, rowToolIds),
      )
    : [];
  const startTool =
    summary?.status === "ready" && !actionsBlocked
      ? firstLaunchableTool(list)
      : null;
  const recommendation = summary ? recommendHomeAction(summary) : null;

  const destinations: Record<HomeDestination, () => void> = {
    tools: () => onOpenTools(),
    services: () => onOpenServices(recommendation?.toolId ?? undefined),
    mcp: () => onOpenMcp(),
  };

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("home.pageLabel")}
      tabIndex={-1}
      className="flex min-w-0 flex-col gap-4 outline-none"
    >
      {refreshFailed ? (
        <HomeRefreshNotice
          retrying={retrying}
          retryButtonRef={retryButtonRef}
          onRetry={retryHome}
        />
      ) : null}

      {checkUnavailable ? (
        <HomeUnavailableState
          retrying={retrying}
          retryButtonRef={retryButtonRef}
          onRetry={retryHome}
        />
      ) : check.isPending || !summary ? (
        <HomeStatusChecking />
      ) : (
        <HomeStatusLine
          summary={summary}
          recommendation={recommendation}
          startTool={startTool}
          checkedAt={check.checkedAt}
          rechecking={check.isFetching || check.isCheckingConnections}
          checkingConnections={check.isCheckingConnections}
          connectionCheckError={check.connectionCheckError}
          onRecheck={() => void check.recheck()}
          onReview={(destination) => destinations[destination]()}
          onStart={launch.openTool}
          onOpenUpdates={onOpenTools}
        />
      )}

      {/* The status line counts; this list names each finding with its next
          step. Findings Home already states elsewhere are left out. */}
      {summary && !checkUnavailable && findings.length > 0 ? (
        <Card padding="sm" role="region" aria-label={t("home.health.title")}>
          <QuickCheckResolutionGuide
            summary={{ ...summary, items: findings }}
            tools={list}
            onOpenTools={onOpenTools}
            onOpenServices={onOpenServices}
            onOpenMcp={onOpenMcp}
          />
        </Card>
      ) : null}

      {summary && !checkUnavailable && summary.installedCount > 0 ? (
        <HomeToolList
          tools={rows}
          onInstall={onOpenTools}
          onOpenTool={launch.openTool}
          onOpenServices={onOpenServices}
        />
      ) : null}

      <HomeDialogs
        launch={launch}
        actionsBlocked={actionsBlocked}
        retrying={retrying}
        refreshFailed={refreshFailed}
      />
    </div>
  );
}
