import { useTranslation } from "react-i18next";
import type { Tool, ToolId } from "@/entities/tool";
import { QuickCheckResolutionGuide } from "@/features/health";
import {
  busyTools,
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
import { HomeLiveRouting } from "./HomeLiveRouting";
import { HomeStatusLine } from "./HomeStatusLine";
import { HomeToolList } from "./HomeToolList";
import {
  recommendHomeAction,
  type HomeDestination,
} from "./homeRecommendation";
import { connectableTools, coveredByToolRow } from "./homeToolConnection";
import { UpdateAllFailureNotice } from "./UpdateAllFailureNotice";
import { updateAllHintKey } from "./updateAllHint";
import { useHomeReadinessRecovery } from "./useHomeReadinessRecovery";
import { useHomeUpdateAll } from "./useHomeUpdateAll";

export interface HomePageProps {
  onOpenTools: () => void;
  onOpenServices: (toolId?: ToolId) => void;
  onOpenMcp: () => void;
  /** Opens the privacy section of the settings page. */
  onOpenPrivacySettings?: () => void;
}

export function HomePage({
  onOpenTools,
  onOpenServices,
  onOpenMcp,
  onOpenPrivacySettings,
}: HomePageProps) {
  const { t } = useTranslation();
  const {
    check,
    operations,
    pageRef,
    retryButtonRef,
    checkUnavailable,
    refreshFailed,
    sourcesUnavailable,
    sourcesPending,
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
          !coveredByToolRow(item, rowToolIds),
      )
    : [];
  const startTool =
    summary?.status === "ready" && !actionsBlocked
      ? firstLaunchableTool(list)
      : null;
  const recommendation = summary ? recommendHomeAction(summary) : null;
  const outdated = list.filter((tool) => tool.status === "updateAvailable");
  const updateable = outdated.filter((tool) => tool.capabilities.canUpdate);
  const busy = busyTools(operations.data ?? []);
  const readyToUpdate = updateable.filter((tool) => !busy.has(tool.id));
  const updates = useHomeUpdateAll({
    readyToUpdate,
    actionsBlocked,
    refetchOperations: operations.refetch,
  });

  const destinations: Record<HomeDestination, () => void> = {
    tools: () => onOpenTools(),
    services: () => onOpenServices(recommendation?.toolId ?? undefined),
    mcp: () => onOpenMcp(),
  };

  const updateFor = (tool: Tool) =>
    tool.status !== "updateAvailable"
      ? null
      : {
          running: busy.has(tool.id),
          disabled: actionsBlocked || updates.scheduling,
          // A tool this app cannot update itself is reviewed where its
          // installation is explained, not in a dialog that could only refuse.
          onSelect: tool.capabilities.canUpdate
            ? () => updates.start([tool])
            : onOpenTools,
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
        />
      )}

      <HomeLiveRouting onOpenPrivacySettings={onOpenPrivacySettings} />

      {/* The status line counts; this list names each finding with its next
          step. Findings a tool row already shows are left to that row. */}
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
          updateAll={
            outdated.length === 0
              ? null
              : {
                  disabled:
                    actionsBlocked ||
                    readyToUpdate.length === 0 ||
                    updates.scheduling,
                  loading: updates.scheduling && updates.bulk,
                  hintKey: updateAllHintKey({
                    scheduling: updates.scheduling,
                    sourcesUnavailable,
                    sourcesPending,
                    retrying,
                    readyCount: readyToUpdate.length,
                    updateableBusy: updateable.some((tool) =>
                      busy.has(tool.id),
                    ),
                  }),
                  onSelect: () => updates.start(),
                }
          }
          updateFor={updateFor}
          onInstall={onOpenTools}
          onOpenTool={launch.openTool}
          onOpenServices={onOpenServices}
        />
      ) : null}

      {updates.skippedCount > 0 && updates.pending.length === 0 ? (
        <UpdateAllFailureNotice
          failedCount={0}
          startedCount={0}
          skippedCount={updates.skippedCount}
          onReviewSkipped={onOpenTools}
        />
      ) : null}

      <HomeDialogs
        updates={updates}
        launch={launch}
        pageRef={pageRef}
        actionsBlocked={actionsBlocked}
        retrying={retrying}
        refreshFailed={refreshFailed}
        onReviewSkipped={onOpenTools}
      />
    </div>
  );
}
