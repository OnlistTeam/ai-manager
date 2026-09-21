import { Download, PlugZap, RefreshCw } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { ToolId } from "@/entities/tool";
import {
  QuickCheckResolutionGuide,
  summarizeEnvironment,
} from "@/features/health";
import {
  firstLaunchableTool,
  OpenToolModal,
  UpdateConfirmationModal,
  busyTools,
  useToolLaunchFlow,
} from "@/features/tool-management";
import { Card } from "@/shared/ui/Card";
import { SectionHeader } from "@/shared/ui/SectionHeader";
import { EnvironmentHero } from "./EnvironmentHero";
import { EnvironmentHeroSkeleton } from "./EnvironmentHeroSkeleton";
import {
  HomeActionCheckingNotice,
  HomeActionPausedNotice,
  HomeRefreshNotice,
  HomeUnavailableState,
} from "./HomeReadinessNotice";
import {
  recommendHomeAction,
  type HomeDestination,
} from "./homeRecommendation";
import { QuickActions, type QuickAction } from "./QuickActions";
import { UpdateAllFailureNotice } from "./UpdateAllFailureNotice";
import { updateAllHintKey } from "./updateAllHint";
import { useHomeReadinessRecovery } from "./useHomeReadinessRecovery";
import { useHomeUpdateAll } from "./useHomeUpdateAll";

export interface HomePageProps {
  onOpenTools: () => void;
  onOpenServices: (toolId?: ToolId) => void;
  onOpenExtensions: () => void;
}

export function HomePage({
  onOpenTools,
  onOpenServices,
  onOpenExtensions,
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
  const heroPending = check.isPending || !summary;
  const startTool =
    summary?.status === "ready" && !actionsBlocked
      ? firstLaunchableTool(list)
      : null;
  const recommendation = summary ? recommendHomeAction(summary) : null;
  const outdated = list.filter((tool) => tool.status === "updateAvailable");
  const updateable = outdated.filter((tool) => tool.capabilities.canUpdate);
  const busy = busyTools(operations.data ?? []);
  const readyToUpdate = updateable.filter((tool) => !busy.has(tool.id));
  const updateAll = useHomeUpdateAll({
    readyToUpdate,
    actionsBlocked,
    refetchOperations: operations.refetch,
  });
  const updateAllDisabled =
    actionsBlocked || readyToUpdate.length === 0 || updateAll.scheduling;
  const updateHintKey = updateAllHintKey({
    scheduling: updateAll.scheduling,
    sourcesUnavailable,
    sourcesPending,
    retrying,
    readyCount: readyToUpdate.length,
    updateableBusy: updateable.some((tool) => busy.has(tool.id)),
    outdatedCount: outdated.length,
    checkingVersions: check.tools.checkingVersions,
    unverified: summarizeEnvironment(list).unverified,
  });

  const actions: readonly QuickAction[] = [
    {
      id: "install",
      labelKey: "home.quickActions.installTool",
      descriptionKey: "home.quickActions.installDescription",
      icon: Download,
      onSelect: () => onOpenTools(),
    },
    {
      id: "updateAll",
      labelKey: "home.quickActions.updateAll",
      descriptionKey: "home.quickActions.updateDescription",
      icon: RefreshCw,
      disabled: updateAllDisabled,
      hintKey: updateHintKey,
      onSelect: updateAll.start,
    },
    {
      id: "connect",
      labelKey: "home.quickActions.connectService",
      descriptionKey: "home.quickActions.connectDescription",
      icon: PlugZap,
      onSelect: () => onOpenServices(),
    },
  ];

  const destinations: Record<HomeDestination, () => void> = {
    tools: () => onOpenTools(),
    services: () => onOpenServices(),
    extensions: () => onOpenExtensions(),
  };

  return (
    <div
      ref={pageRef}
      role="region"
      aria-label={t("home.pageLabel")}
      tabIndex={-1}
      className="flex min-w-0 flex-col gap-8 outline-none"
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
      ) : heroPending ? (
        <EnvironmentHeroSkeleton
          title={t("home.card.title")}
          label={t("home.card.checking")}
        />
      ) : (
        <EnvironmentHero
          summary={summary}
          recommendation={recommendation}
          startTool={startTool}
          checkedAt={check.checkedAt}
          rechecking={check.isFetching || check.isCheckingConnections}
          checkingConnections={check.isCheckingConnections}
          connectionCheckError={check.connectionCheckError}
          onRecheck={() => void check.recheck()}
          onReview={(destination) => {
            if (destination === "services") {
              onOpenServices(recommendation?.toolId ?? undefined);
              return;
            }
            destinations[destination]();
          }}
          onStart={launch.openTool}
        />
      )}

      {/* The hero only carries the count and the primary action now — it
          stopped naming the specific problem. This list is the one place
          that does, whether there is one attention item or several. */}
      {summary && !checkUnavailable && summary.attentionCount > 0 ? (
        <Card padding="lg" role="region" aria-label={t("home.health.title")}>
          <QuickCheckResolutionGuide
            summary={summary}
            tools={list}
            onOpenTools={onOpenTools}
            onOpenServices={onOpenServices}
            onOpenExtensions={onOpenExtensions}
          />
        </Card>
      ) : null}

      <section className="flex flex-col gap-3">
        <SectionHeader title={t("home.quickActions.title")} />
        <QuickActions actions={actions} />
      </section>

      {updateAll.skippedCount > 0 && updateAll.pending.length === 0 ? (
        <UpdateAllFailureNotice
          failedCount={0}
          startedCount={0}
          skippedCount={updateAll.skippedCount}
          onReviewSkipped={onOpenTools}
        />
      ) : null}

      <UpdateConfirmationModal
        open={updateAll.pending.length > 0}
        tools={updateAll.pending}
        previews={updateAll.previews.data}
        loading={updateAll.previews.isPending}
        refreshing={
          updateAll.previews.isFetching && updateAll.previews.data !== undefined
        }
        previewError={updateAll.previews.error}
        submitting={updateAll.scheduling}
        mutationError={null}
        returnFocusFallbackRef={pageRef}
        actionPaused={actionsBlocked || !updateAll.stillAuthorized}
        bulk
        notice={
          <>
            {retrying && !refreshFailed ? <HomeActionCheckingNotice /> : null}
            {refreshFailed ? <HomeActionPausedNotice /> : null}
            {updateAll.failure ? (
              <UpdateAllFailureNotice
                {...updateAll.failure}
                skippedCount={updateAll.skippedCount}
                onReviewSkipped={onOpenTools}
              />
            ) : null}
          </>
        }
        onOpenChange={updateAll.setOpen}
        onRefresh={updateAll.refresh}
        onConfirm={updateAll.confirm}
      />

      <OpenToolModal
        tool={launch.tool}
        busy={launch.busy}
        confirmDisabled={actionsBlocked}
        error={launch.error}
        providerRecovery={launch.providerRecovery}
        notice={
          retrying && !refreshFailed ? (
            <HomeActionCheckingNotice />
          ) : refreshFailed ? (
            <HomeActionPausedNotice />
          ) : null
        }
        onOpenChange={launch.onOpenChange}
        onLaunchDefault={() => {
          if (!actionsBlocked) launch.confirm("default");
        }}
        onChooseFolder={() => {
          if (!actionsBlocked) launch.confirm("choose");
        }}
      />
    </div>
  );
}
