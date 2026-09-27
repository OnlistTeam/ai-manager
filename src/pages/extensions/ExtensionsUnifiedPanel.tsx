import { Puzzle } from "lucide-react";
import { useTranslation } from "react-i18next";
import type { Operation } from "@/entities/operation";
import type { useSkillUpdates } from "@/entities/skill-update";
import {
  useUpdateSkill,
  type ExtensionScopeOption,
  type ExtensionTab,
  type UnifiedExtensions,
} from "@/features/extension-management";
import {
  TaskAvailabilityNotice,
  type useTaskAvailability,
} from "@/features/task-center";
import { ToolOperationProgress } from "@/features/tool-management";
import { Button } from "@/shared/ui/Button";
import { EmptyState } from "@/shared/ui/EmptyState";
import { scopeTabId } from "@/shared/ui/ScopeTabs";
import { ExtensionsRefreshNotice } from "./ExtensionsRefreshNotice";
import { ExtensionsSkeleton } from "./ExtensionsSkeleton";
import { ExtensionsUnifiedList } from "./ExtensionsUnifiedList";
import { KIND_PANEL_ID, KIND_TAB_PREFIX } from "./ExtensionsKindTabs";
import { SkillUpdatesNotice } from "./SkillUpdatesNotice";
import type { useExtensionsDialogsState } from "./useExtensionsDialogsState";

interface ExtensionsUnifiedPanelProps {
  tab: ExtensionTab;
  fixedKind: boolean;
  /** Every installed app for this kind, including ones not integrated yet. */
  targets: readonly ExtensionScopeOption[];
  unified: UnifiedExtensions;
  operations: readonly Operation[];
  task: ReturnType<typeof useTaskAvailability>;
  skillUpdates: ReturnType<typeof useSkillUpdates>;
  actionsBlocked: boolean;
  dialogs: ReturnType<typeof useExtensionsDialogsState>;
}

/**
 * Skills and MCP: one list for every app (ADR-0048). The switches on each
 * row replace the per-app tabs, so the only page state left is the list.
 */
export function ExtensionsUnifiedPanel({
  tab,
  fixedKind,
  targets,
  unified,
  operations,
  task,
  skillUpdates,
  actionsBlocked,
  dialogs,
}: ExtensionsUnifiedPanelProps) {
  const { t } = useTranslation();
  // An update is one click, like CC Switch: it is queued as a task, keeps a
  // restore copy, and can be followed in Activity.
  const updateSkill = useUpdateSkill();
  const supported = targets.filter((target) => target.supported);
  const notIntegrated = targets
    .filter((target) => !target.supported)
    .map((target) => target.name);

  return (
    <section
      id={KIND_PANEL_ID}
      role={fixedKind ? undefined : "tabpanel"}
      aria-labelledby={
        fixedKind ? undefined : scopeTabId(KIND_TAB_PREFIX, tab.kind)
      }
      className="flex flex-col gap-4"
    >
      {supported.length === 0 ? (
        <EmptyState
          icon={Puzzle}
          title={t("extensions.noTools.title")}
          description={[
            t(tab.explainerKey),
            t("extensions.noTools.description"),
          ]
            .filter(Boolean)
            .join(" ")}
        />
      ) : (
        <>
          <TaskAvailabilityNotice
            state={task.state}
            refreshing={task.operations.isFetching}
            onRetry={() => void task.operations.refetch()}
          />

          {tab.kind === "skill" ? (
            <SkillUpdatesNotice updates={skillUpdates} />
          ) : null}

          {unified.initiallyLoading ? (
            <ExtensionsSkeleton label={t("extensions.loading")} />
          ) : null}

          {unified.unavailable ? (
            <div
              role="alert"
              aria-label={t("extensions.error.title")}
              aria-busy={unified.isFetching || undefined}
            >
              <EmptyState
                icon={Puzzle}
                title={t("extensions.error.title")}
                description={t("extensions.error.description")}
                action={
                  <Button
                    loading={unified.isFetching}
                    onClick={unified.refetch}
                  >
                    {t("extensions.refresh")}
                  </Button>
                }
              />
            </div>
          ) : null}

          {unified.refreshFailed ? (
            <ExtensionsRefreshNotice
              refreshing={unified.isFetching}
              onRetry={unified.refetch}
            />
          ) : null}

          {operations.map((operation) => (
            <ToolOperationProgress
              key={operation.id}
              operation={operation}
              toolName={
                supported.find(
                  (target) =>
                    target.scope.id ===
                    (operation.tool ?? operation.desktopApp),
                )?.name ?? ""
              }
            />
          ))}

          {unified.dataAvailable ? (
            unified.rows.length === 0 ? (
              <EmptyState
                icon={Puzzle}
                title={t(tab.emptyTitleKey)}
                description={t(tab.emptyDescriptionKey)}
              />
            ) : (
              <ExtensionsUnifiedList
                kind={tab.kind}
                rows={unified.rows}
                targets={supported}
                operations={operations}
                actionsBlocked={
                  actionsBlocked ||
                  unified.isFetching ||
                  unified.isError ||
                  task.actionsBlocked
                }
                skillUpdateIds={
                  new Set(skillUpdates.data?.map((update) => update.id) ?? [])
                }
                onRemove={dialogs.openRemoval}
                onEdit={dialogs.openMcpEdit}
                onUpdate={(skill) => {
                  if (skill.scope.kind !== "tool") return;
                  updateSkill.mutate({
                    tool: skill.scope.id,
                    skillId: skill.id,
                    name: skill.name,
                  });
                }}
              />
            )
          ) : null}
        </>
      )}

      {notIntegrated.length > 0 ? (
        <p className="text-caption text-content-muted">
          {t("extensions.list.notIntegrated", {
            tools: notIntegrated,
            kind: t(tab.titleKey),
          })}
        </p>
      ) : null}
    </section>
  );
}
